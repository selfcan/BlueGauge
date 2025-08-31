use crate::{
    BluetoothDevicesInfo, UserEvent,
    bluetooth::{
        ble::{BluetoothLEDeviceUpdate, get_ble_device_from_address, watch_ble_devices_async},
        btc::{get_btc_device_from_address, get_pnp_devices_info, watch_btc_devices_status_async},
        info::{BluetoothInfo, BluetoothType},
    },
    notify::app_notify,
};

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

use anyhow::{Result, anyhow};
use log::info;
use winit::event_loop::EventLoopProxy;

pub struct Watcher {
    watch_handles: Option<[JoinHandle<Result<(), anyhow::Error>>; 3]>,
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
) -> [JoinHandle<Result<(), anyhow::Error>>; 3] {
    info!("The watch thread is started.");

    let watch_btc_battery_handle = spawn_watch!(watch_btc_devices_battery, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_btc_status_handle = spawn_watch!(watch_btc_devices_status, bluetooth_devices_info, exit_flag, restart_flag, proxy);
    let watch_ble_handle = spawn_watch!(watch_ble_devices, bluetooth_devices_info, exit_flag, restart_flag, proxy);

    [
        watch_ble_handle,
        watch_btc_battery_handle,
        watch_btc_status_handle,
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

        let pnp_devices = get_pnp_devices_info()?;

        let mut need_update = false;
        for btc_device in original_btc_devices.into_iter() {
            if restart_flag.swap(false, Ordering::Relaxed) {
                break;
            }
            if let Some(pnp_info) = pnp_devices.get(&btc_device.address) {
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

// fn watch_devices_count_changed()
