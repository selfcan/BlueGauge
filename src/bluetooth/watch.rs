use crate::{
    BluetoothDevicesInfo, UserEvent,
    bluetooth::{
        ble::{process_ble_device, watch_ble_devices_async},
        btc::{
            get_btc_info_device_frome_address, watch_btc_devices_battery,
            watch_btc_devices_status_async,
        },
        info::BluetoothInfo,
    },
    notify::NotifyEvent,
};

use std::{
    collections::hash_map::Entry,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use log::{info, warn};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use windows::{
    Devices::{
        Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice},
        Enumeration::{
            DeviceInformation, DeviceInformationUpdate, DeviceWatcher, DeviceWatcherStatus,
        },
    },
    Foundation::TypedEventHandler,
    core::{HSTRING, Ref},
};
use winit::event_loop::EventLoopProxy;

type WatchHandle = JoinHandle<Result<(), anyhow::Error>>;

macro_rules! spawn_watch {
    ($func:expr, $info:expr, $exit_flag:expr, $restart_flag:expr, $proxy:expr) => {{
        let info = Arc::clone(&$info);
        let exit_flag = Arc::clone(&$exit_flag);
        let restart_flag = Arc::clone(&$restart_flag);
        let proxy = $proxy.clone();

        tokio::spawn(async move { $func(info, &exit_flag, &restart_flag, proxy).await })
    }};
}

pub struct Watcher {
    watch_handles: Option<[WatchHandle; 4]>,
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: Arc<AtomicBool>,
    restart_flag: Arc<AtomicUsize>,
    proxy: EventLoopProxy<UserEvent>,
}

impl Watcher {
    pub fn new(
        bluetooth_devices_info: BluetoothDevicesInfo,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let exit_flag = Arc::new(AtomicBool::new(false));
        let restart_flag = Arc::new(AtomicUsize::new(0));
        Self {
            watch_handles: None,
            bluetooth_devices_info,
            exit_flag,
            restart_flag,
            proxy,
        }
    }

    pub fn start(&mut self) {
        info!("Starting the watch thread...");

        let watch_handles = self.watch_loop();

        self.watch_handles = Some(watch_handles);
    }

    pub fn stop(&self) {
        info!("Stopping the watch thread...");
        self.exit_flag.store(true, Ordering::Relaxed);
        self.restart_flag.store(0, Ordering::Relaxed);
    }

    #[rustfmt::skip]
    fn watch_loop(&self) -> [WatchHandle; 4] {
        info!("The watch thread is started.");

        let watch_btc_battery_handle = spawn_watch!(watch_btc_devices_battery, self.bluetooth_devices_info, self.exit_flag, self.restart_flag, self.proxy);
        let watch_btc_status_handle = spawn_watch!(watch_btc_devices_status_async, self.bluetooth_devices_info, self.exit_flag, self.restart_flag, self.proxy);
        let watch_ble_handle = spawn_watch!(watch_ble_devices_async, self.bluetooth_devices_info, self.exit_flag, self.restart_flag, self.proxy);
        let watch_bt_presence_handle = spawn_watch!(watch_bt_presence_async, self.bluetooth_devices_info, self.exit_flag, self.restart_flag, self.proxy);

        [
            watch_ble_handle,
            watch_btc_battery_handle,
            watch_btc_status_handle,
            watch_bt_presence_handle,
        ]
    }
}

#[derive(PartialEq, Eq)]
enum BluetoothPresence {
    Added,
    Removed,
}

async fn check_presence_async(
    is_ble: bool,
    presence: BluetoothPresence,
    id: HSTRING,
    tx: Sender<(BluetoothInfo, BluetoothPresence)>,
) -> Result<()> {
    match presence {
        BluetoothPresence::Added => {
            if is_ble {
                let ble_device = BluetoothLEDevice::FromIdAsync(&id)?.await?;
                match process_ble_device(&ble_device).await {
                    Ok(ble_info) => {
                        let _ = tx.send((ble_info, presence)).await;
                    }
                    Err(e) => {
                        let name = ble_device
                            .Name()
                            .map_or_else(|_| "Unknown name".to_owned(), |n| n.to_string());
                        warn!("BLE [{name}]: Failed to get info: {e}");
                    }
                }
            } else {
                let btc_device = BluetoothDevice::FromIdAsync(&id)?.await?;
                let process_btc_device = |btc_device: &BluetoothDevice| {
                    let btc_name = btc_device.Name()?.to_string();
                    let btc_address = btc_device.BluetoothAddress()?;
                    let btc_status =
                        btc_device.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
                    // [!] 等待Pnp设备初始化后方可获取经典蓝牙信息
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    get_btc_info_device_frome_address(btc_name.clone(), btc_address, btc_status)
                };
                match process_btc_device(&btc_device) {
                    Ok(btc_info) => {
                        let _ = tx.send((btc_info, presence)).await;
                    }
                    Err(e) => {
                        let name = btc_device
                            .Name()
                            .map_or_else(|_| "Unknown name".to_owned(), |n| n.to_string());
                        warn!("BTC [{name}]: Failed to get info: {e}");
                    }
                }
            };
        }
        BluetoothPresence::Removed => {
            let remove_device_address = if is_ble {
                let device = BluetoothLEDevice::FromIdAsync(&id)?.await?;
                device.BluetoothAddress()?
            } else {
                let device = BluetoothDevice::FromIdAsync(&id)?.await?;
                device.BluetoothAddress()?
            };
            let remove_device_info = BluetoothInfo {
                address: remove_device_address,
                ..Default::default()
            };
            let _ = tx.send((remove_device_info, presence)).await;
        }
    }

    Ok(())
}

macro_rules! create_handler {
    ($tx:ident, $arg_type:ty, $is_ble:expr, $presence:expr) => {{
        let handler_tx = $tx.clone();
        TypedEventHandler::new(
            move |_watcher: Ref<DeviceWatcher>, event_info: Ref<$arg_type>| {
                if let Some(info) = event_info.as_ref() {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| -> windows::core::Error { e.into() })?;

                    let id = info.Id()?;

                    let result = rt.block_on(async {
                        check_presence_async($is_ble, $presence, id, handler_tx.clone()).await
                    });

                    result.map_err(|e| {
                        windows::core::Error::new(
                            windows::core::HRESULT(0x80004005u32 as i32), // E_FAIL
                            e.to_string(),
                        )
                    })?;
                }
                Ok(())
            },
        )
    }};
}

fn start_bt_presence_watch(device_watcher: &DeviceWatcher) -> Result<()> {
    let status = device_watcher.Status()?;

    if matches!(
        status,
        DeviceWatcherStatus::Aborted | DeviceWatcherStatus::Created | DeviceWatcherStatus::Stopped
    ) {
        device_watcher
            .Start()
            .with_context(|| "Failed to start watch for the DeviceWatcher")
    } else {
        Err(anyhow!(
            "DeviceWatcher is already started or starting, current status: {:?}",
            status
        ))
    }
}

fn stop_bt_presence_watch(device_watcher: &DeviceWatcher) -> Result<()> {
    let status = device_watcher.Status()?;

    if matches!(
        status,
        DeviceWatcherStatus::Aborted
            | DeviceWatcherStatus::EnumerationCompleted
            | DeviceWatcherStatus::Started
    ) {
        device_watcher
            .Stop()
            .with_context(|| "Failed to stop watch for the DeviceWatcher")
    } else {
        Err(anyhow!(
            "DeviceWatcher is already stoped or stoping, current status: {:?}",
            status
        ))
    }
}

#[rustfmt::skip]
async fn watch_bt_presence_async(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicUsize>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let btc_filter = BluetoothDevice::GetDeviceSelector()?;
    let btc_watcher = DeviceInformation::CreateWatcherAqsFilter(&btc_filter)?;
    let btc_tokens = {
        let added_handler = create_handler!(tx, DeviceInformation, false, BluetoothPresence::Added);
        let removed_handler = create_handler!(tx, DeviceInformationUpdate, false, BluetoothPresence::Removed);
        let btc_watch_added_token = btc_watcher.Added(&added_handler)?;
        let btc_watch_removed_token = btc_watcher.Removed(&removed_handler)?;
        [btc_watch_added_token, btc_watch_removed_token]
    };

    let ble_filter = BluetoothLEDevice::GetDeviceSelector()?;
    let ble_watcher = DeviceInformation::CreateWatcherAqsFilter(&ble_filter)?;
    let ble_tokens = {
        let added_handler = create_handler!(tx, DeviceInformation, true, BluetoothPresence::Added);
        let removed_handler = create_handler!(tx, DeviceInformationUpdate, true, BluetoothPresence::Removed);
        let ble_watch_added_token = ble_watcher.Added(&added_handler)?;
        let ble_watch_removed_token = ble_watcher.Removed(&removed_handler)?;
        [ble_watch_added_token, ble_watch_removed_token]
    };

    start_bt_presence_watch(&btc_watcher)?;
    start_bt_presence_watch(&ble_watcher)?;

    scopeguard::defer! {
        info!("Release the watching of presence in the devices");

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

        stop_bt_presence_watch(&btc_watcher).unwrap();
        stop_bt_presence_watch(&ble_watcher).unwrap();
    }

    while !exit_flag.load(Ordering::Relaxed) {
        tokio::select! {
            maybe_update = rx.recv() => {
                if let Some((info, presence)) = maybe_update {
                    let update_event = |presence: BluetoothPresence, name: String| {
                        // 设备添加/移除后，所有监听增加或移除设备
                        restart_flag.fetch_add(1, Ordering::Relaxed);
                        // 更新托盘信息
                        let _ = proxy.send_event(UserEvent::UnpdatTray);
                        // 因 Watcher 无 Config，需传递给有通知配置的 APP 结构体
                        match presence {
                            BluetoothPresence::Added => {
                                info!("[{name}]: New Bluetooth Device Connected");
                                let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::Added(name)));
                            }
                            BluetoothPresence::Removed => {
                                 info!("[{name}]: Bluetooth Device Removed");
                                let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::Removed(name)));
                            }
                        }
                    };

                    if let Entry::Vacant(e) = bluetooth_devices_info.lock().unwrap().entry(info.address) {
                        let name = info.name.clone();
                        e.insert(info);
                        update_event(presence, name);
                    } else {
                        match presence {
                            BluetoothPresence::Added => (), // 原设备未被移除
                            BluetoothPresence::Removed => {
                                let removed_info = bluetooth_devices_info.lock().unwrap().remove(&info.address);
                                let name = removed_info.map_or("Unknown name".to_owned(), |i| i.name);
                                update_event(presence, name);
                            }
                        }
                    }
                } else {
                    return Err(anyhow!("Channel closed while watching Bluetooth presence"));
                }
            }
            _ = async {
                while !exit_flag.load(Ordering::Relaxed) {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            } => info!("Watch Bluetooth Presence was cancelled by exit flag."),
        }
    }

    Ok(())
}
