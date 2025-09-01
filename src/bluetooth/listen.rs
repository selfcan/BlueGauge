use crate::{
    bluetooth::{
        ble::{get_ble_device_from_address, watch_ble_devices_async, process_ble_device, BluetoothLEDeviceUpdate},
        btc::{get_btc_device_from_address, get_btc_info_device_frome_address, get_pnp_devices, get_pnp_devices_info, watch_btc_devices_status_async},
        info::{BluetoothInfo, BluetoothType},
    }, notify::app_notify, BluetoothDevicesInfo, UserEvent
};

use std::{
    sync::{
        atomic::{AtomicBool, Ordering}, Arc
    }, thread::JoinHandle
};

use anyhow::{Result, anyhow};
use log::{info, error};
use windows::{
    core::Ref,
    Devices::{
        Bluetooth::{BluetoothDevice, BluetoothLEDevice, BluetoothConnectionStatus},
        Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher},
    }, Foundation::TypedEventHandler
};
use winit::event_loop::EventLoopProxy;

pub struct Watcher {
    watch_handles: Option<[JoinHandle<Result<(), anyhow::Error>>; 4]>,
    exit_flag: Arc<AtomicBool>,
    restart_flag: Arc<AtomicBool>,
}

impl Watcher {
    pub fn start(devices: BluetoothDevicesInfo, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        info!("Starting the watch thread...");

        let exit_flag = Arc::new(AtomicBool::new(false));
        let restart_flag = Arc::new(AtomicBool::new(false));

        let thread_exit_flag = Arc::clone(&exit_flag);
        let thread_restart_flag = Arc::clone(&restart_flag);
        let watch_handles = watch_loop(devices, proxy, thread_exit_flag, thread_restart_flag);

        Ok(Self {
            watch_handles: Some(watch_handles),
            exit_flag,
            restart_flag,
        })
    }

    pub fn stop(mut self) {
        if let Some(handles) = self.watch_handles.take() {
            info!("Stopping the watch thread...");

            self.restart_flag.store(false, Ordering::Relaxed);
            self.exit_flag.store(true, Ordering::Relaxed);

            handles
                .into_iter()
                .filter_map(|handle| handle.join().expect("Failed to stop watch threads").err())
                .for_each(|e| app_notify(format!("An error occurred while watching {e}")));
        }
    }
}

macro_rules! spawn_watch {
    ($func:expr, $info:expr, $exit_flag:expr, $restart_flag:expr, $proxy:expr) => {
        std::thread::spawn({
            let info = Arc::clone(&$info);
            let exit_flag = Arc::clone(&$exit_flag);
            let restart_flag = Arc::clone(&$restart_flag);
            let proxy = $proxy.clone();
            move || $func(info, &exit_flag, &restart_flag, proxy)
        })
    };
}

#[rustfmt::skip]
fn watch_loop(
    bluetooth_devices_info: BluetoothDevicesInfo,
    proxy: EventLoopProxy<UserEvent>,
    exit_flag: Arc<AtomicBool>,
    restart_flag: Arc<AtomicBool>,
) -> [JoinHandle<Result<(), anyhow::Error>>; 4] {
    info!("The watch thread is started.");

    let watch_btc_battery_handle = spawn_watch!(watch_btc_devices_battery, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_btc_status_handle = spawn_watch!(watch_btc_devices_status, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_ble_handle = spawn_watch!(watch_ble_devices, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_bt_presence_handle = spawn_watch!(watch_bt_presence, bluetooth_devices_info, exit_flag, restart_flag, proxy);

    [
        watch_ble_handle,
        watch_btc_battery_handle,
        watch_btc_status_handle,
        watch_bt_presence_handle,
    ]
}

fn watch_btc_devices_battery(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicBool>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    while !exit_flag.load(Ordering::Relaxed) {
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if exit_flag.load(Ordering::Relaxed) {
                return Ok(());
            }
            if restart_flag.swap(false, Ordering::Relaxed) {
                break;
            }
        }

        let original_btc_devices = bluetooth_devices_info
            .lock()
            .unwrap()
            .values()
            .cloned()
            .filter(|info| matches!(info.r#type, BluetoothType::Classic(_)))
            .collect::<Vec<_>>();

        let pnp_devices = get_pnp_devices()?;
        let pnp_devices_info = get_pnp_devices_info(pnp_devices)?;

        // let pnp_devices = get_pnp_devices_info()?;

        let mut need_update = false;
        for btc_device in original_btc_devices.into_iter() {
            if restart_flag.swap(false, Ordering::Relaxed) {
                break;
            }
            if let Some(pnp_info) = pnp_devices_info.get(&btc_device.address) {
                if pnp_info.battery != btc_device.battery {
                    bluetooth_devices_info.lock().unwrap().insert(
                        pnp_info.address,
                        BluetoothInfo {
                            battery: pnp_info.battery,
                            ..btc_device
                        },
                    );
                    need_update = true;
                }
            }
        }

        if need_update {
            let _ = proxy.send_event(UserEvent::UnpdatTray);
        }
    }

    Ok(())
}

fn watch_btc_devices_status(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicBool>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    while !exit_flag.load(Ordering::Relaxed) {
        let btc_devices = bluetooth_devices_info
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(address, info)| {
                matches!(info.r#type, BluetoothType::Classic(_))
                    .then(|| get_btc_device_from_address(*address).ok())
                    .flatten()
            })
            .collect::<Vec<_>>();

        let runtime = tokio::runtime::Runtime::new().expect("Failed to create a Tokio runtime");
        match runtime.block_on(watch_btc_devices_status_async(
            btc_devices,
            exit_flag,
            restart_flag,
        )) {
            Ok(Some((address, status))) => {
                if let Some(update_device) =
                    bluetooth_devices_info.lock().unwrap().get_mut(&address)
                {
                    if update_device.status != status {
                        info!(
                            "BTC [{}]: Status -> {}",
                            update_device.name, update_device.status
                        );
                        update_device.status = status;
                        let _ = proxy.send_event(UserEvent::UnpdatTray);
                    }
                }
            }
            Err(e) => return Err(anyhow!("BTC devices status watch failed: {e}")),
            Ok(None) => (),
        }
    }

    Ok(())
}

fn watch_ble_devices(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicBool>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    while !exit_flag.load(Ordering::Relaxed) {
        let ble_devices = bluetooth_devices_info
            .lock()
            .unwrap()
            .values()
            .cloned()
            .filter_map(|info| {
                (info.r#type == BluetoothType::LowEnergy)
                    .then(|| get_ble_device_from_address(info.address).ok())
                    .flatten()
            })
            .collect::<Vec<_>>();

        let runtime = tokio::runtime::Runtime::new().expect("Failed to create a Tokio runtime");
        match runtime.block_on(watch_ble_devices_async(
            ble_devices,
            exit_flag,
            restart_flag,
        )) {
            Ok(Some(update)) => {
                let mut devices = bluetooth_devices_info.lock().unwrap();
                let need_update_ble_info = match update {
                    BluetoothLEDeviceUpdate::BatteryLevel(address, battery) => devices
                        .get(&address)
                        .filter(|i| i.battery != battery)
                        .cloned()
                        .map(|mut info| {
                            info.battery = battery;
                            info
                        }),
                    BluetoothLEDeviceUpdate::ConnectionStatus(address, status) => devices
                        .get(&address)
                        .filter(|i| i.status != status)
                        .cloned()
                        .map(|mut info| {
                            info.status = status;
                            info
                        }),
                };

                if let Some(ble_info) = need_update_ble_info {
                    info!(
                        "BLE [{}]: Status -> {}, Battery -> {}",
                        ble_info.name, ble_info.status, ble_info.battery
                    );

                    devices.insert(ble_info.address, ble_info.clone());
                    drop(devices);

                    let _ = proxy.send_event(UserEvent::UnpdatTray);
                }
            }
            Err(e) => return Err(anyhow!("BLE devices watch failed: {e}")),
            Ok(None) => (),
        }
    }

    Ok(())
}

macro_rules! create_handler {
    // $tx: 接收一个标识符，代表 channel sender
    // $arg_name: 接收一个标识符，代表闭包参数名
    // $arg_type: 接收一个类型
    // $event_flag: 接收一个表达式，代表发送的布尔值
    ($tx:ident, $arg_type:ty, $is_ble:expr) => {
        {
            let handler_tx = $tx.clone();
            TypedEventHandler::new(
                move |_watcher: Ref<DeviceWatcher>, event_info: Ref<$arg_type>| {
                    if let Some(info) = event_info.as_ref() {
                        if $is_ble {
                            let ble_device = BluetoothLEDevice::FromIdAsync(&info.Id()?)?.get()?;
                            match process_ble_device(&ble_device) {
                                Ok(ble_info) => {
                                    let _ = handler_tx.try_send(ble_info);
                                }
                                Err(e) => error!("Failed to get BLE devices info: {e}")
                            }
                        } else {
                            let btc_device = BluetoothDevice::FromIdAsync(&info.Id()?)?.get()?;
                            let btc_name = btc_device.Name()?.to_string();
                            let btc_address = btc_device.BluetoothAddress()?;
                            let btc_status = btc_device.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
                            match get_btc_info_device_frome_address(btc_name, btc_address, btc_status) {
                                Ok(btc_info) => {
                                    let _ = handler_tx.try_send(btc_info);
                                }
                                Err(e) => error!("Failed to get BTC devices info: {e}")
                            }
                        }
                    }
                    Ok(())
                },
            )
        }
    };
}

fn watch_bt_presence(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicBool>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create a Tokio runtime");
    runtime.block_on(watch_bt_presence_async(
        bluetooth_devices_info,
        exit_flag,
        restart_flag,
        proxy,
    ))
}

async fn watch_bt_presence_async(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicBool>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    println!("开始监听设备的存在变化");
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let btc_filter = BluetoothDevice::GetDeviceSelector()?;
    let btc_watcher = DeviceInformation::CreateWatcherAqsFilter(&btc_filter)?;
    let btc_tokens = {
        let added_handler = create_handler!(tx, DeviceInformation, false);
        let removed_handler = create_handler!(tx, DeviceInformationUpdate, false);
        let btc_watch_added_token = btc_watcher.Added(&added_handler)?;
        let btc_watch_removed_token = btc_watcher.Removed(&removed_handler)?;
        [btc_watch_added_token, btc_watch_removed_token]
    };

    let ble_filter = BluetoothLEDevice::GetDeviceSelector()?;
    let ble_watcher = DeviceInformation::CreateWatcherAqsFilter(&ble_filter)?;
    let ble_tokens = {
        let added_handler = create_handler!(tx, DeviceInformation, true);
        let removed_handler = create_handler!(tx, DeviceInformationUpdate, true);
        let ble_watch_added_token = ble_watcher.Added(&added_handler)?;
        let ble_watch_removed_token = ble_watcher.Removed(&removed_handler)?;
        [ble_watch_added_token, ble_watch_removed_token]
    };

    scopeguard::defer! {
        println!("释放了设备存在变化的监听");
        btc_tokens.into_iter().enumerate().for_each(|(index, token)| match index {
            0 => { let _ = btc_watcher.RemoveAdded(token); },
            1 => { let _ = btc_watcher.RemoveRemoved(token); },
            _ => ()
        });
        ble_tokens.into_iter().enumerate().for_each(|(index, token)| match index {
            0 => { let _ = ble_watcher.RemoveAdded(token); },
            1 => { let _ = ble_watcher.RemoveRemoved(token); },
            _ => ()
        });
    }

    tokio::select! {
        maybe_update = rx.recv() => {
            if let Some(info) = maybe_update {
                if bluetooth_devices_info.lock().unwrap().remove(&info.address).is_some() {
                    // 如找到，说明原设备被移除
                    // let _ = proxy.send_event(UserEvent::NotifyDeviceChanged(DeviceChanged::Removed(info.name)));
                    info!("原设备被移除：{}", info.name);
                } else {
                    // 如未找到，说明新设备被添加
                    // let _ = proxy.send_event(UserEvent::NotifyDeviceChanged(DeviceChanged::Added(info.name)));
                    info!("新设备被添加：{}", info.name);
                }

                restart_flag.store(true, Ordering::Relaxed);
                let _ = proxy.send_event(UserEvent::UnpdatTray);
            } else {
                return Err(anyhow!("Channel closed while watching Bluetooth presence"))
            }
        },
        _ = async {
            loop {
                println!("监听设备的存在变化延迟一秒");
                if exit_flag.load(Ordering::Relaxed) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        } => {
            info!("Watch BTC Status was cancelled by exit flag.");
        }
    }

    Ok(())
}
