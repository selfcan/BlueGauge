use crate::{
    BluetoothDevicesInfo, UserEvent,
    bluetooth::{
        ble::{
            BluetoothLEDeviceUpdate, get_ble_device_from_address, process_ble_device,
            watch_ble_devices_async,
        },
        btc::{
            get_btc_device_from_address, get_btc_info_device_frome_address, get_pnp_devices,
            get_pnp_devices_info, watch_btc_devices_status_async,
        },
        info::{BluetoothInfo, BluetoothType},
    },
    notify::notify,
};

use std::{
    collections::hash_map::Entry,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

use anyhow::{Result, anyhow};
use log::{info, warn};
use windows::{
    Devices::{
        Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice},
        Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher},
    },
    Foundation::TypedEventHandler,
    core::Ref,
};
use winit::event_loop::EventLoopProxy;

pub struct Watcher {
    watch_handles: Option<[(JoinHandle<Result<(), anyhow::Error>>, &'static str); 4]>,
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
                .filter_map(|(handle, handle_name)| {
                    handle
                        .join()
                        .expect("Failed to stop watch threads")
                        .err()
                        .map(|e| (handle_name, e))
                })
                .for_each(|(n, e)| notify(format!("An error occurred while watching {n}: {e}")));
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
) -> [(JoinHandle<Result<(), anyhow::Error>>, &'static str); 4] {
    info!("The watch thread is started.");

    let watch_btc_battery_handle = spawn_watch!(watch_btc_devices_battery, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_btc_status_handle = spawn_watch!(watch_btc_devices_status, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_ble_handle = spawn_watch!(watch_ble_devices, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_bt_presence_handle = spawn_watch!(watch_bt_presence, bluetooth_devices_info, exit_flag, restart_flag, proxy);

    [
        (watch_ble_handle, "Watch BLE Handle"),
        (watch_btc_battery_handle, "Watch BTC Battery Handle"),
        (watch_btc_status_handle, "Watch BTC Status Handle"),
        (watch_bt_presence_handle, "Watch Bluetooth presence Handle"),
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
            .filter(|info| matches!(info.r#type, BluetoothType::Classic(_)))
            .cloned()
            .collect::<Vec<_>>();

        let pnp_devices = get_pnp_devices()?;
        let pnp_devices_info = get_pnp_devices_info(pnp_devices)?;

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

#[derive(PartialEq, Eq)]
enum BluetoothPresence {
    Added,
    Removed,
}

macro_rules! create_handler {
    // $tx: 接收一个标识符，代表 channel sender
    // $arg_type: 接收一个类型
    // $event_flag: 接收一个表达式，代表发送的布尔值
    ($tx:ident, $arg_type:ty, $is_ble:expr, $presence:expr) => {{
        let handler_tx = $tx.clone();
        TypedEventHandler::new(
            move |_watcher: Ref<DeviceWatcher>, event_info: Ref<$arg_type>| {
                if let Some(info) = event_info.as_ref() {
                    match $presence {
                        BluetoothPresence::Added => {
                            if $is_ble {
                                let ble_device =
                                    BluetoothLEDevice::FromIdAsync(&info.Id()?)?.get()?;
                                match process_ble_device(&ble_device) {
                                    Ok(ble_info) => {
                                        let _ = handler_tx.try_send((ble_info, $presence));
                                    }
                                    Err(e) => warn!("Failed to get BLE info: {e}"),
                                }
                            } else {
                                let btc_device =
                                    BluetoothDevice::FromIdAsync(&info.Id()?)?.get()?;
                                let process_ble_device = |btc_device: &BluetoothDevice| {
                                    let btc_address = btc_device.BluetoothAddress()?;
                                    let btc_name = btc_device.Name()?.to_string();
                                    let btc_status = btc_device.ConnectionStatus()?
                                        == BluetoothConnectionStatus::Connected;
                                    get_btc_info_device_frome_address(
                                        btc_name.clone(),
                                        btc_address,
                                        btc_status,
                                    )
                                };
                                match process_ble_device(&btc_device) {
                                    Ok(btc_info) => {
                                        let _ = handler_tx.try_send((btc_info, $presence));
                                    }
                                    Err(e) => warn!("Failed to get BTC info: {e}"),
                                }
                            };
                        }
                        BluetoothPresence::Removed => {
                            let remove_device_address = if $is_ble {
                                let device = BluetoothLEDevice::FromIdAsync(&info.Id()?)?.get()?;
                                device.BluetoothAddress()?
                            } else {
                                let device = BluetoothDevice::FromIdAsync(&info.Id()?)?.get()?;
                                device.BluetoothAddress()?
                            };
                            let remove_device_info = BluetoothInfo {
                                address: remove_device_address,
                                ..Default::default()
                            };
                            let _ = handler_tx.try_send((remove_device_info, $presence));
                        }
                    }
                }
                Ok(())
            },
        )
    }};
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
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let btc_filter = BluetoothDevice::GetDeviceSelector()?;
    let btc_watcher = DeviceInformation::CreateWatcherAqsFilter(&btc_filter)?;
    let btc_tokens = {
        let added_handler = create_handler!(tx, DeviceInformation, false, BluetoothPresence::Added);
        let removed_handler = create_handler!(
            tx,
            DeviceInformationUpdate,
            false,
            BluetoothPresence::Removed
        );
        let btc_watch_added_token = btc_watcher.Added(&added_handler)?;
        let btc_watch_removed_token = btc_watcher.Removed(&removed_handler)?;
        [btc_watch_added_token, btc_watch_removed_token]
    };

    let ble_filter = BluetoothLEDevice::GetDeviceSelector()?;
    let ble_watcher = DeviceInformation::CreateWatcherAqsFilter(&ble_filter)?;
    let ble_tokens = {
        let added_handler = create_handler!(tx, DeviceInformation, true, BluetoothPresence::Added);
        let removed_handler = create_handler!(
            tx,
            DeviceInformationUpdate,
            true,
            BluetoothPresence::Removed
        );
        let ble_watch_added_token = ble_watcher.Added(&added_handler)?;
        let ble_watch_removed_token = ble_watcher.Removed(&removed_handler)?;
        [ble_watch_added_token, ble_watch_removed_token]
    };

    let _ = btc_watcher.Start();
    let _ = ble_watcher.Start();

    scopeguard::defer! {
        println!("Release the watching of presence in the devices");
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
        let _ = btc_watcher.Stop();
        let _ = ble_watcher.Stop();
    }

    while !exit_flag.load(Ordering::Relaxed) {
        if let Some((info, presence)) = rx.recv().await {
            let update_event = |presence: BluetoothPresence| {
                restart_flag.store(true, Ordering::Relaxed);
                let _ = proxy.send_event(UserEvent::UnpdatTray);
                // Watcher无通知配置，需传递有通知配置的代理的APP结构体
                match presence {
                    BluetoothPresence::Added => {
                        // let _ = proxy.send_event(UserEvent::NotifyDeviceAdded(info.name));
                    }
                    BluetoothPresence::Removed => {
                        // let _ = proxy.send_event(UserEvent::NotifyDeviceRemoved(info.name));
                    }
                }
            };

            if let Entry::Vacant(e) = bluetooth_devices_info.lock().unwrap().entry(info.address) {
                info!("[{}]: New Bluetooth Device Connected", info.name);
                e.insert(info);
                update_event(presence);
            } else {
                match presence {
                    BluetoothPresence::Added => (), // 原设备未被移除
                    BluetoothPresence::Removed => {
                        let removed_info =
                            bluetooth_devices_info.lock().unwrap().remove(&info.address);
                        update_event(presence);
                        info!(
                            "[{}]: Bluetooth Device Removed",
                            removed_info.map_or("Unknown name".to_owned(), |i| i.name)
                        );
                    }
                }
            }
        } else {
            return Err(anyhow!("Channel closed while watching Bluetooth presence"));
        }
    }

    info!("Watch Bluetooth Presence was cancelled by exit flag.");

    Ok(())
}
