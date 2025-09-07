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
use futures::{StreamExt, future::join_all};
use log::{info, warn};
use tokio::sync::{Mutex, mpsc::Sender};
use windows::{
    Devices::Bluetooth::{
        BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::{
            GattCharacteristic, GattCharacteristicProperties, GattCharacteristicUuids,
            GattServiceUuids, GattValueChangedEventArgs,
        },
    },
    Devices::Enumeration::DeviceInformation,
    Foundation::TypedEventHandler,
    Storage::Streams::DataReader,
    core::GUID,
};
use winit::event_loop::EventLoopProxy;

pub async fn find_ble_devices() -> Result<Vec<BluetoothLEDevice>> {
    let ble_aqs_filter = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;

    let ble_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&ble_aqs_filter)?
        .await
        .with_context(|| "Failed to find BLE from AqsFilter")?;

    let ble_devices = futures::stream::iter(ble_devices_info)
        .filter_map(|device_info| async move {
            let device_id = device_info.Id().ok()?;
            BluetoothLEDevice::FromIdAsync(&device_id).ok()?.await.ok()
        })
        .collect::<Vec<_>>()
        .await;

    Ok(ble_devices)
}

pub async fn get_ble_device_from_address(address: u64) -> Result<BluetoothLEDevice> {
    BluetoothLEDevice::FromBluetoothAddressAsync(address)?
        .await
        .map_err(|e| anyhow!("Failed to find BLE from ({address}) - {e}"))
}

pub async fn get_ble_devices_info(
    ble_devices: &[BluetoothLEDevice],
) -> Result<HashMap<u64, BluetoothInfo>> {
    let mut devices_info: HashMap<u64, BluetoothInfo> = HashMap::new();

    let futures = ble_devices.iter().map(process_ble_device);

    let results = join_all(futures).await;

    results.into_iter().for_each(|result| match result {
        Ok(info) => {
            devices_info.insert(info.address, info);
        }
        Err(e) => warn!("{e}"),
    });

    Ok(devices_info)
}

pub async fn process_ble_device(ble_device: &BluetoothLEDevice) -> Result<BluetoothInfo> {
    let name = ble_device.Name()?.to_string();

    let status = ble_device
        .ConnectionStatus()
        .map(|status| status == BluetoothConnectionStatus::Connected)
        .with_context(|| "Failed to get BLE connected status")?;

    let address = ble_device.BluetoothAddress()?;

    let battery = get_ble_battery_level(ble_device)
        .await
        .map_err(|e| anyhow!("Failed to get BLE Battery Level: {e}"))?;

    Ok(BluetoothInfo {
        name,
        battery,
        status,
        address,
        r#type: BluetoothType::LowEnergy,
    })
}

async fn get_ble_battery_gatt_char(ble_device: &BluetoothLEDevice) -> Result<GattCharacteristic> {
    // 0000180F-0000-1000-8000-00805F9B34FB
    let battery_services_uuid: GUID = GattServiceUuids::Battery()?;
    // 00002A19-0000-1000-8000-00805F9B34FB
    let battery_level_uuid: GUID = GattCharacteristicUuids::BatteryLevel()?;

    let battery_gatt_services = ble_device
        .GetGattServicesForUuidAsync(battery_services_uuid)?
        .await?
        .Services()
        .map_err(|e| anyhow!("Failed to get BLE Battery Gatt Services: {e}"))?;

    let battery_gatt_service = battery_gatt_services
        .into_iter()
        .next()
        .ok_or(anyhow!("Failed to get BLE Battery Gatt Service"))?; // [*] 手机蓝牙无电量服务;

    let battery_gatt_chars = battery_gatt_service
        .GetCharacteristicsForUuidAsync(battery_level_uuid)?
        .await?
        .Characteristics()
        .map_err(|e| anyhow!("Failed to get BLE Battery Gatt Characteristics: {e}"))?;

    let battery_gatt_char = battery_gatt_chars
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Failed to get BLE Battery Gatt Characteristic"))?;

    let battery_gatt_char_uuid = battery_gatt_char.Uuid()?;

    if battery_gatt_char_uuid == battery_level_uuid {
        Ok(battery_gatt_char)
    } else {
        Err(anyhow!(
            "Failed to match BLE level UUID:\n{battery_gatt_char_uuid:?}:\n{battery_level_uuid:?}"
        ))
    }
}

pub async fn get_ble_battery_level(ble_device: &BluetoothLEDevice) -> Result<u8> {
    let battery_gatt_char = get_ble_battery_gatt_char(ble_device).await?;
    let buffer = battery_gatt_char.ReadValueAsync()?.await?.Value()?;
    let reader = DataReader::FromBuffer(&buffer)?;
    reader
        .ReadByte()
        .map_err(|e| anyhow!("Failed to read battery byte: {e}"))
}

#[derive(Debug)]
enum BluetoothLEUpdate {
    BatteryLevel(/* Address */ u64, u8),
    ConnectionStatus(/* Address */ u64, bool),
}

type WatchBLEGuard = (BluetoothLEDevice, GattCharacteristic, i64, i64);

async fn watch_ble_device(
    ble_address: u64,
    ble_device: BluetoothLEDevice,
    tx: Sender<BluetoothLEUpdate>,
) -> Result<WatchBLEGuard> {
    let battery_gatt_char = get_ble_battery_gatt_char(&ble_device).await?;

    let char_properties = battery_gatt_char.CharacteristicProperties()?;

    if !char_properties.contains(GattCharacteristicProperties::Notify) {
        return Err(anyhow!("Battery level does not support notifications"));
    }

    let tx_status = tx.clone();
    let connection_status_token = {
        let handler = TypedEventHandler::new(
            move |sender: windows::core::Ref<BluetoothLEDevice>, _args| {
                if let Some(ble) = sender.as_ref() {
                    let status = ble.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
                    let _ = tx_status
                        .try_send(BluetoothLEUpdate::ConnectionStatus(ble_address, status));
                }
                Ok(())
            },
        );
        ble_device.ConnectionStatusChanged(&handler)?
    };

    let tx_battery = tx.clone();
    let battery_token = {
        let handler = TypedEventHandler::new(
            move |_, args: windows::core::Ref<GattValueChangedEventArgs>| {
                if let Ok(args) = args.ok() {
                    let value = args.CharacteristicValue()?;
                    let reader = DataReader::FromBuffer(&value)?;
                    let battery = reader.ReadByte()?;
                    let _ =
                        tx_battery.try_send(BluetoothLEUpdate::BatteryLevel(ble_address, battery));
                }
                Ok(())
            },
        );
        battery_gatt_char.ValueChanged(&handler)?
    };

    Ok((
        ble_device,
        battery_gatt_char,
        connection_status_token,
        battery_token,
    ))
}

pub async fn watch_ble_devices_async(
    bluetooth_devices_info: BluetoothDevicesInfo,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicUsize>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    let mut local_generation = 0;

    let original_ble_devices_address = Arc::new(Mutex::new(HashSet::new()));

    let addresses_to_process = bluetooth_devices_info
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, info)| info.r#type == BluetoothType::LowEnergy)
        .map(|(&address, _)| address)
        .collect::<Vec<_>>();

    let ble_devices = futures::stream::iter(addresses_to_process)
        .filter_map(|address| {
            let original_ble_devices_address = original_ble_devices_address.clone();
            async move {
                match get_ble_device_from_address(address).await {
                    Ok(ble_device) => {
                        original_ble_devices_address.lock().await.insert(address);
                        Some((address, ble_device))
                    }
                    Err(_) => None,
                }
            }
        })
        .collect::<Vec<_>>()
        .await;

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let mut guard = scopeguard::guard(HashMap::<u64, WatchBLEGuard>::new(), |map| {
        for (device, char, connection_status_token, battery_token) in map.into_values() {
            let _ = device.RemoveConnectionStatusChanged(connection_status_token);
            let _ = char.RemoveValueChanged(battery_token);
        }
    });

    
    for (ble_address, ble_device) in ble_devices {
        let watch_btc_guard = watch_ble_device(ble_address, ble_device, tx.clone()).await?;

        guard.insert(ble_address, watch_btc_guard);
    }

    while !exit_flag.load(Ordering::Relaxed) {
        tokio::select! {
            maybe_update = rx.recv() => {
                if let Some(update) = maybe_update {
                    let mut devices = bluetooth_devices_info.lock().unwrap();
                    let need_update_ble_info = match update {
                        BluetoothLEUpdate::BatteryLevel(address, battery) => devices
                            .get(&address)
                            .filter(|i| i.battery != battery)
                            .cloned()
                            .map(|mut info| {
                                info!("BLE [{}]: Battery -> {battery}", info.name);
                                let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::LowBattery(
                                    info.name.clone(),
                                    battery,
                                    info.address,
                                )));
                                info.battery = battery;
                                info
                            }),
                        BluetoothLEUpdate::ConnectionStatus(address, status) => devices
                            .get(&address)
                            .filter(|i| i.status != status)
                            .cloned()
                            .map(|mut info| {
                                info!("BLE [{}]: Status -> {status}", info.name);
                                let notify_event = if status {
                                    NotifyEvent::Reconnect(info.name.clone())
                                } else {
                                    NotifyEvent::Disconnect(info.name.clone())
                                };
                                let _ = proxy.send_event(UserEvent::Notify(notify_event));
                                info.status = status;
                                info
                            }),
                    };

                    if let Some(ble_info) = need_update_ble_info {
                        devices.insert(ble_info.address, ble_info.clone());
                        drop(devices);

                        let _ = proxy.send_event(UserEvent::UnpdatTray);
                    }
                } else {
                    return Err(anyhow!("Channel closed while watching BLE devcies"));
                }
            },
            _ = async {
                let original_ble_devices_address = Arc::clone(&original_ble_devices_address);
                loop {
                    if exit_flag.load(Ordering::Relaxed) {
                        info!("Watch BLE was cancelled by exit flag.");
                        break;
                    }

                    let current_generation = restart_flag.load(Ordering::Relaxed);
                    if local_generation < current_generation {
                        info!("Watch BLE restart by restart flag.");
                        local_generation = current_generation;

                        let current_ble_devices_address = bluetooth_devices_info
                            .lock()
                            .unwrap()
                            .iter()
                            .filter(|(_, info)| matches!(info.r#type, BluetoothType::LowEnergy))
                            .map(|(&address, _)| address)
                            .collect::<HashSet<_>>();

                        let original_ble_devices_address_clone = original_ble_devices_address.lock().await.clone();

                        let removed_devices = original_ble_devices_address_clone
                            .difference(&current_ble_devices_address)
                            .cloned()
                            .collect::<Vec<_>>();

                        let added_devices = current_ble_devices_address
                            .difference(&original_ble_devices_address_clone)
                            .cloned()
                            .collect::<Vec<_>>();

                        for removed_device in removed_devices {
                            guard.remove(&removed_device);
                            original_ble_devices_address.lock().await.remove(&removed_device);
                        }

                        for added_device_address in added_devices {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let ble_device = get_ble_device_from_address(added_device_address)
                                .await
                                .expect("Failed to get BLE Device from address");
                            let watch_ble_guard = watch_ble_device(added_device_address, ble_device, tx.clone())
                                .await
                                .expect("Failed to watch BLE Device");
                            guard.insert(added_device_address, watch_ble_guard);
                            original_ble_devices_address.lock().await.insert(added_device_address);
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            } => return Ok(()),
        }
    }

    Ok(())
}
