use crate::{
    BluetoothDevicesInfo, UserEvent,
    bluetooth::info::{BluetoothInfo, BluetoothType},
    notify::NotifyEvent,
};

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use log::{error, info, warn};
use tokio::sync::{Mutex, mpsc::Sender};
use windows::{
    Devices::{
        Bluetooth::{BluetoothConnectionStatus, BluetoothDevice},
        Enumeration::DeviceInformation,
    },
    Foundation::TypedEventHandler,
};
use windows_pnp::{PnpDeviceNodeInfo, PnpDevicePropertyValue, PnpEnumerator, PnpFilter};
use windows_sys::{
    Wdk::Devices::Bluetooth::DEVPKEY_Bluetooth_DeviceAddress,
    Win32::{Devices::DeviceAndDriverInstallation::GUID_DEVCLASS_SYSTEM, Foundation::DEVPROPKEY},
};
use winit::event_loop::EventLoopProxy;

#[allow(non_upper_case_globals)]
const DEVPKEY_Bluetooth_Battery: DEVPROPKEY = DEVPROPKEY {
    fmtid: windows_sys::core::GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5),
    pid: 2,
};
const BT_INSTANCE_ID: &str = "BTHENUM\\";

pub struct PnpDeviceInfo {
    pub battery: u8,
    pub instance_id: String,
}

pub async fn find_btc_devices() -> Result<Vec<BluetoothDevice>> {
    let btc_aqs_filter = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;

    let btc_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&btc_aqs_filter)?
        .await
        .with_context(|| "Failed to find BTC from AqsFilter")?;

    let btc_devices = futures::stream::iter(btc_devices_info)
        .filter_map(|device_info| async move {
            let device_id = device_info.Id().ok()?;
            BluetoothDevice::FromIdAsync(&device_id).ok()?.await.ok()
        })
        .collect::<Vec<_>>()
        .await;

    Ok(btc_devices)
}

pub async fn get_btc_device_from_address(address: u64) -> Result<BluetoothDevice> {
    BluetoothDevice::FromBluetoothAddressAsync(address)?
        .await
        .with_context(|| format!("Failed to find BTC device from ({address})"))
}

pub async fn get_btc_devices_info(
    btc_devices: &[BluetoothDevice],
) -> Result<HashMap<u64, BluetoothInfo>> {
    // [!] 获取Pnp设备可能出错（初始化可能失败），需重试多次避开错误
    let pnp_devices_info = {
        let max_retries = 2;
        let mut attempts = 0;

        loop {
            let pnp_devices = get_pnp_devices().await?;
            match get_pnp_devices_info(pnp_devices).await {
                Ok(info) => break info,
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_retries {
                        return Err(anyhow!(
                            "Trying to enumerate the pnp device twice failed: {e}"
                        )); // 达到最大重试次数，返回错误
                    }
                    error!(
                        "Failed to get Bluetooth device information: {e}, try again after 2 seconds... (try {attempts}/{max_retries})"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    };

    let mut devices_info: HashMap<u64, BluetoothInfo> = HashMap::new();

    btc_devices.iter().for_each(|btc_device| {
        match process_btc_device(btc_device, &pnp_devices_info) {
            Ok(i) => {
                devices_info.insert(i.address, i);
            }
            Err(e) => warn!("{e}"),
        };
    });

    Ok(devices_info)
}

fn process_btc_device(
    btc_device: &BluetoothDevice,
    pnp_devices_info: &HashMap<u64, PnpDeviceInfo>,
) -> Result<BluetoothInfo> {
    let btc_name = btc_device.Name()?.to_string().trim().to_owned();

    let btc_address = btc_device.BluetoothAddress()?;

    let (pnp_instance_id, btc_battery) = pnp_devices_info
        .get(&btc_address)
        .map(|i| (i.instance_id.clone(), i.battery))
        .ok_or_else(|| anyhow!("BTC [{btc_name}]: No matching BTC in Pnp devices"))?;

    let btc_status = btc_device.ConnectionStatus()? == BluetoothConnectionStatus::Connected;

    Ok(BluetoothInfo {
        name: btc_name,
        battery: btc_battery,
        status: btc_status,
        address: btc_address,
        r#type: BluetoothType::Classic(pnp_instance_id),
    })
}

pub async fn get_btc_info_device_frome_address(
    name: String,
    address: u64,
    status: bool,
) -> Result<BluetoothInfo> {
    let btc_address_bytes = format!("{address:012X}");

    let pnp_device_node_info = tokio::task::spawn_blocking(move || {
        PnpEnumerator::enumerate_present_devices_and_filter_by_device_setup_class(
            GUID_DEVCLASS_SYSTEM,
            PnpFilter::Contains(&[BT_INSTANCE_ID.to_owned(), btc_address_bytes]),
        )
        .map_err(|e| anyhow!("Failed to enumerate pnp device ({address}) - {e:?}"))
    })
    .await??;

    if pnp_device_node_info.is_empty() {
        return Err(anyhow!("No enumeration to PNP device ({address:012X})"));
    }

    let pnp_device_info = get_pnp_devices_info(pnp_device_node_info)
        .await
        .with_context(|| "Failed to get pnp device info")?
        .remove(&address)
        .ok_or_else(|| anyhow!("No matching BTC info in pnp device info"))?;

    Ok(BluetoothInfo {
        name,
        battery: pnp_device_info.battery,
        status,
        address,
        r#type: BluetoothType::Classic(pnp_device_info.instance_id),
    })
}

pub async fn get_pnp_devices() -> Result<Vec<PnpDeviceNodeInfo>> {
    tokio::task::spawn_blocking(move || {
        PnpEnumerator::enumerate_present_devices_and_filter_by_device_setup_class(
            GUID_DEVCLASS_SYSTEM,
            PnpFilter::Contains(&[BT_INSTANCE_ID.to_owned()]),
        )
        .map_err(|e| anyhow!("Failed to enumerate pnp devices - {e:?}"))
    })
    .await?
}

pub async fn get_pnp_devices_from_devices_instance_id(
    devices_instance_id: Vec<String>,
) -> Result<Vec<PnpDeviceNodeInfo>> {
    tokio::task::spawn_blocking(move || {
        PnpEnumerator::enumerate_present_devices_and_filter_by_device_setup_class(
            GUID_DEVCLASS_SYSTEM,
            PnpFilter::Equal(&devices_instance_id),
        )
        .map_err(|e| anyhow!("Failed to enumerate pnp devices from devices instance id - {e:?}"))
    })
    .await?
}

pub async fn get_pnp_devices_info(
    pnp_devices_node_info: Vec<PnpDeviceNodeInfo>,
) -> Result<HashMap<u64, PnpDeviceInfo>> {
    let mut pnp_devices_info: HashMap<u64, PnpDeviceInfo> = HashMap::new();

    for pnp_device_node_info in pnp_devices_node_info.into_iter() {
        if let Some(mut props) = pnp_device_node_info.device_instance_properties {
            let battery = props
                .remove(&DEVPKEY_Bluetooth_Battery.into())
                .and_then(|value| match value {
                    PnpDevicePropertyValue::Byte(v) => Some(v),
                    _ => None,
                });

            let address = props
                .remove(&DEVPKEY_Bluetooth_DeviceAddress.into())
                .and_then(|value| match value {
                    PnpDevicePropertyValue::String(v) => u64::from_str_radix(&v, 16).ok(),
                    _ => None,
                });

            if let (Some(address), Some(battery)) = (address, battery) {
                pnp_devices_info.insert(
                    address,
                    PnpDeviceInfo {
                        // address,
                        battery,
                        instance_id: pnp_device_node_info.device_instance_id,
                    },
                );
            }
        }
    }

    Ok(pnp_devices_info)
}

pub async fn watch_btc_devices_battery(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicUsize>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    let mut local_generation = 0;

    while !exit_flag.load(Ordering::Relaxed) {
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            if exit_flag.load(Ordering::Relaxed) {
                return Ok(());
            }

            let current_generation = restart_flag.load(Ordering::Relaxed);
            if local_generation < current_generation {
                info!("Watch BTC Batttery restart by restart flag.");
                local_generation = current_generation;
                break;
            }
        }

        let original_btc_devices = bluetooth_devices_info.lock().unwrap().clone();
        let original_btc_devices_instance_id = original_btc_devices
            .values()
            .filter_map(|info| {
                if let BluetoothType::Classic(id) = &info.r#type {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let pnp_devices =
            get_pnp_devices_from_devices_instance_id(original_btc_devices_instance_id).await?;
        let pnp_devices_info = get_pnp_devices_info(pnp_devices).await?;

        let mut need_update = false;
        for (btc_address, btc_device) in original_btc_devices.into_iter() {
            // 刷新时退出循环
            let current_generation = restart_flag.load(Ordering::Relaxed);
            if local_generation < current_generation {
                info!("Watch BTC Batttery restart by restart flag.");
                local_generation = current_generation;
                break;
            }

            if let Some(pnp_info) = pnp_devices_info.get(&btc_address) {
                let pnp_battery = pnp_info.battery;
                if pnp_battery != btc_device.battery {
                    let name = btc_device.name.clone();
                    bluetooth_devices_info.lock().unwrap().insert(
                        btc_address,
                        BluetoothInfo {
                            battery: pnp_battery,
                            ..btc_device
                        },
                    );
                    need_update = true;
                    info!("BTC [{name}]: Battery -> {}", pnp_battery);
                    let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::LowBattery(
                        name,
                        pnp_battery,
                        btc_address,
                    )));
                }
            }
        }

        if need_update {
            let _ = proxy.send_event(UserEvent::UnpdatTray);
        }
    }

    Ok(())
}

type WatchBTCGuard = (BluetoothDevice, i64);

async fn watch_btc_device_status(
    btc_address: u64,
    btc_device: BluetoothDevice,
    tx: Sender<(u64, bool)>,
) -> Result<WatchBTCGuard> {
    let tx_status = tx.clone();
    let connection_status_token = {
        let handler =
            TypedEventHandler::new(move |sender: windows::core::Ref<BluetoothDevice>, _args| {
                if let Some(btc) = sender.as_ref() {
                    let status = btc.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
                    let _ = tx_status.try_send((btc_address, status));
                }
                Ok(())
            });
        btc_device.ConnectionStatusChanged(&handler)?
    };

    Ok((btc_device, connection_status_token))
}

pub async fn watch_btc_devices_status_async(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicUsize>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    let mut local_generation = 0;

    let original_btc_devices_address = Arc::new(Mutex::new(HashSet::new()));

    let addresses_to_process = bluetooth_devices_info
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, info)| matches!(info.r#type, BluetoothType::Classic(_)))
        .map(|(&address, _)| address)
        .collect::<Vec<_>>();

    let btc_devices = futures::stream::iter(addresses_to_process)
        .filter_map(|address| {
            let original_btc_devices_address = original_btc_devices_address.clone();
            async move {
                match get_btc_device_from_address(address).await {
                    Ok(btc_device) => {
                        original_btc_devices_address.lock().await.insert(address);
                        Some((address, btc_device))
                    }
                    Err(_) => None,
                }
            }
        })
        .collect::<Vec<_>>()
        .await;

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let mut guard = scopeguard::guard(HashMap::<u64, WatchBTCGuard>::new(), |map| {
        for (device, connection_status_token) in map.into_values() {
            let _ = device.RemoveConnectionStatusChanged(connection_status_token);
        }
    });

    for (btc_address, btc_device) in btc_devices {
        let watch_btc_guard = watch_btc_device_status(btc_address, btc_device, tx.clone()).await?;

        guard.insert(btc_address, watch_btc_guard);
    }

    loop {
        tokio::select! {
            maybe_update = rx.recv() => {
                let Some((address, status)) = maybe_update else {
                    return Err(anyhow!("Channel closed while watching BTC devices status"));
                };
                let mut devices = bluetooth_devices_info.lock().unwrap();
                if let Some(update_device) = devices.get_mut(&address) {
                    if update_device.status != status {
                        info!("BTC [{}]: Status -> {status}", update_device.name);
                        let notify_event = if status {
                            NotifyEvent::Reconnect(update_device.name.clone())
                        } else {
                            NotifyEvent::Disconnect(update_device.name.clone())
                        };
                        update_device.status = status;
                        drop(devices);
                        let _ = proxy.send_event(UserEvent::Notify(notify_event));
                        let _ = proxy.send_event(UserEvent::UnpdatTray);
                    }
                };

            },
            _ = async {
                loop {
                    if exit_flag.load(Ordering::Relaxed) {
                        info!("Watch BTC Status was cancelled by exit flag.");
                        break;
                    }

                    let current_generation = restart_flag.load(Ordering::Relaxed);
                    if local_generation < current_generation {
                        info!("Watch BTC Status restart by restart flag.");
                        local_generation = current_generation;

                        let current_btc_devices_address = bluetooth_devices_info
                            .lock()
                            .unwrap()
                            .iter()
                            .filter(|(_, info)| matches!(info.r#type, BluetoothType::Classic(_)))
                            .map(|(&address, _)| address)
                            .collect::<HashSet<_>>();

                        let original_btc_devices_address_clone = original_btc_devices_address.lock().await.clone();

                        let removed_devices = original_btc_devices_address_clone
                            .difference(&current_btc_devices_address)
                            .cloned()
                            .collect::<Vec<_>>();

                        let added_devices = current_btc_devices_address
                            .difference(&original_btc_devices_address_clone)
                            .cloned()
                            .collect::<Vec<_>>();

                        for removed_device in removed_devices {
                            guard.remove(&removed_device);
                            original_btc_devices_address.lock().await.remove(&removed_device);
                        }

                        for added_device_address in added_devices {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let Ok(btc_device) = get_btc_device_from_address(added_device_address).await else {
                                // 移除错误设备
                                bluetooth_devices_info.lock().unwrap().remove(&added_device_address);
                                warn!("Failed to get added BTC Device from address");
                                continue;
                            };

                            let name = btc_device.Name().map_or("Unknown name".to_owned(), |n| n.to_string());

                            match watch_btc_device_status(added_device_address, btc_device, tx.clone()).await  {
                                Ok(watch_ble_guard) => {
                                    guard.insert(added_device_address, watch_ble_guard);
                                    original_btc_devices_address.lock().await.insert(added_device_address);
                                },
                                Err(e) => {
                                    // 移除错误设备
                                    bluetooth_devices_info.lock().unwrap().remove(&added_device_address);
                                    warn!("BTC [{name}]: Failed to watch added BTC Device - {e}");
                                }
                            }
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            } => return Ok(()),
        }
    }
}
