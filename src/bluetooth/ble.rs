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
use log::{warn, info};
use tokio::sync::mpsc::Sender;
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

pub fn find_ble_devices() -> Result<Vec<BluetoothLEDevice>> {
    let ble_aqs_filter = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;

    let ble_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&ble_aqs_filter)?
        .GetResults()
        .with_context(|| "Failed to find BLE from AqsFilter")?;

    let ble_devices = ble_devices_info
        .into_iter()
        .filter_map(|device_info| {
            BluetoothLEDevice::FromIdAsync(&device_info.Id().ok()?)
                .ok()?
                .GetResults()
                .ok()
        })
        .collect::<Vec<_>>();

    Ok(ble_devices)
}

pub fn get_ble_device_from_address(address: u64) -> Result<BluetoothLEDevice> {
    BluetoothLEDevice::FromBluetoothAddressAsync(address)?
        .GetResults()
        .map_err(|e| anyhow!("Failed to find BLE from ({address}) - {e}"))
}

pub fn get_ble_devices_info(
    ble_devices: &[BluetoothLEDevice],
) -> Result<HashMap<u64, BluetoothInfo>> {
    let mut devices_info: HashMap<u64, BluetoothInfo> = HashMap::new();

    let results = ble_devices.iter().map(process_ble_device);

    results.for_each(|r_ble_info| match r_ble_info {
        Ok(i) => {
            devices_info.insert(i.address, i);
        }
        Err(e) => warn!("{e}"),
    });

    Ok(devices_info)
}

pub fn process_ble_device(ble_device: &BluetoothLEDevice) -> Result<BluetoothInfo> {
    let name = ble_device.Name()?.to_string();

    let battery = get_ble_battery_level(ble_device)
        .map_err(|e| anyhow!("Failed to get BLE Battery Level: {e}"))?;

    let status = ble_device
        .ConnectionStatus()
        .map(|status| status == BluetoothConnectionStatus::Connected)
        .with_context(|| "Failed to get BLE connected status")?;

    let address = ble_device.BluetoothAddress()?;

    Ok(BluetoothInfo {
        name,
        battery,
        status,
        address,
        r#type: BluetoothType::LowEnergy,
    })
}

fn get_ble_battery_gatt_char(ble_device: &BluetoothLEDevice) -> Result<GattCharacteristic> {
    // 0000180F-0000-1000-8000-00805F9B34FB
    let battery_services_uuid: GUID = GattServiceUuids::Battery()?;
    // 00002A19-0000-1000-8000-00805F9B34FB
    let battery_level_uuid: GUID = GattCharacteristicUuids::BatteryLevel()?;

    let battery_gatt_services = ble_device
        .GetGattServicesForUuidAsync(battery_services_uuid)?
        .GetResults()?
        .Services()
        .map_err(|e| anyhow!("Failed to get BLE Battery Gatt Services: {e}"))?;

    let battery_gatt_service = battery_gatt_services
        .into_iter()
        .next()
        .ok_or(anyhow!("Failed to get BLE Battery Gatt Service"))?; // [*] 手机蓝牙无电量服务;

    let battery_gatt_chars = battery_gatt_service
        .GetCharacteristicsForUuidAsync(battery_level_uuid)?
        .GetResults()?
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

pub fn get_ble_battery_level(ble_device: &BluetoothLEDevice) -> Result<u8> {
    let battery_gatt_char = get_ble_battery_gatt_char(ble_device)?;
    let buffer = battery_gatt_char.ReadValueAsync()?.GetResults()?.Value()?;
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

fn watch_ble_device(
    ble_address: u64,
    ble_device: BluetoothLEDevice,
    tx: Sender<BluetoothLEUpdate>,
) -> Result<WatchBLEGuard> {
    let battery_gatt_char = get_ble_battery_gatt_char(&ble_device)?;

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

    let mut original_ble_devices_address = HashSet::new();

    let ble_devices = bluetooth_devices_info
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(&address, info)| {
            (info.r#type == BluetoothType::LowEnergy)
                .then(|| get_ble_device_from_address(address).ok())
                .flatten()
                .map(|ble_device| {
                    original_ble_devices_address.insert(address);
                    (address, ble_device)
                })
        })
        .collect::<Vec<_>>();

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let mut guard = scopeguard::guard(HashMap::<u64, WatchBLEGuard>::new(), |map| {
        for (device, char, connection_status_token, battery_token) in map.into_values() {
            let _ = device.RemoveConnectionStatusChanged(connection_status_token);
            let _ = char.RemoveValueChanged(battery_token);
        }
    });

    for (address, ble_device) in ble_devices {
        let watch_ble_guard = watch_ble_device(address, ble_device, tx.clone())?;

        guard.insert(address, watch_ble_guard);
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

                        let removed_devices = original_ble_devices_address
                            .difference(&current_ble_devices_address)
                            .cloned()
                            .collect::<Vec<_>>();

                        let added_devices = current_ble_devices_address
                            .difference(&original_ble_devices_address)
                            .cloned()
                            .collect::<Vec<_>>();

                        for removed_device in removed_devices {
                            guard.remove(&removed_device);
                            original_ble_devices_address.remove(&removed_device);
                        }

                        for added_device_address in added_devices {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let ble_device = get_ble_device_from_address(added_device_address)
                                .expect("Failed to get BLE Device from address");
                            let watch_ble_guard = watch_ble_device(added_device_address, ble_device, tx.clone())
                                .expect("Failed to watch BLE Device");
                            guard.insert(added_device_address, watch_ble_guard);
                            original_ble_devices_address.insert(added_device_address);
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            } => return Ok(()),
        }
    }

    Ok(())
}
