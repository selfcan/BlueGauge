use crate::bluetooth::info::{BluetoothInfo, BluetoothType};

use std::{
    collections::HashSet,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, Result, anyhow};
use log::{error, info};
use scopeguard::defer;
use windows::{
    Devices::Bluetooth::{
        BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::{
            GattCharacteristicProperties,
            GattCharacteristicUuids,
            // GattClientCharacteristicConfigurationDescriptorValue, GattCommunicationStatus,
            GattServiceUuids,
            GattValueChangedEventArgs,
        },
    },
    Devices::Enumeration::DeviceInformation,
    Foundation::TypedEventHandler,
    Storage::Streams::DataReader,
    core::GUID,
};

pub fn find_ble_devices() -> Result<Vec<BluetoothLEDevice>> {
    let ble_aqs_filter = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;

    let ble_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&ble_aqs_filter)?
        .get()
        .with_context(|| "Faled to find Bluetooth Low Energy from all devices")?;

    let ble_devices = ble_devices_info
        .into_iter()
        .filter_map(|device_info| {
            BluetoothLEDevice::FromIdAsync(&device_info.Id().ok()?)
                .ok()?
                .get()
                .ok()
        })
        .collect::<Vec<_>>();

    Ok(ble_devices)
}

pub fn find_ble_device(address: u64) -> Result<BluetoothLEDevice> {
    BluetoothLEDevice::FromBluetoothAddressAsync(address)?
        .get()
        .map_err(|e| anyhow!("Failed to find ble ({address}) - {e}"))
}

pub fn get_ble_info(ble_devices: &[BluetoothLEDevice]) -> Result<HashSet<BluetoothInfo>> {
    let mut devices_info: HashSet<BluetoothInfo> = HashSet::new();

    let results = ble_devices.iter().map(process_ble_device);

    results.for_each(|r_ble_info| {
        let _ = r_ble_info
            .inspect_err(|e| error!("{e}"))
            .is_ok_and(|bt_info| devices_info.insert(bt_info));
    });

    Ok(devices_info)
}

pub fn process_ble_device(ble_device: &BluetoothLEDevice) -> Result<BluetoothInfo> {
    let name = ble_device.Name()?.to_string();

    let battery = get_ble_battery_level(ble_device)
        .map_err(|e| anyhow!("Failed to get '{name}'BLE Battery Level: {e}"))?;

    let status = ble_device
        .ConnectionStatus()
        .map(|status| status == BluetoothConnectionStatus::Connected)
        .with_context(|| format!("Failed to get BLE connected status: {name}"))?;

    let address = ble_device.BluetoothAddress()?;

    Ok(BluetoothInfo {
        name,
        battery,
        status,
        address,
        r#type: BluetoothType::LowEnergy,
    })
}

pub fn get_ble_battery_level(ble_device: &BluetoothLEDevice) -> Result<u8> {
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
        .ok_or(anyhow!("Failed to get BLE Battery Gatt Service"))?; // 手机蓝牙无电量服务;

    let battery_gatt_chars = battery_gatt_service
        .GetCharacteristicsForUuidAsync(battery_level_uuid)?
        .get()?
        .Characteristics()
        .map_err(|e| anyhow!("Failed to get BLE Battery Gatt Characteristics: {e}"))?;

    let battery_gatt_char = battery_gatt_chars
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Failed to get BLE Battery Gatt Characteristic"))?;

    match battery_gatt_char.Uuid()? == battery_level_uuid {
        true => {
            let buffer = battery_gatt_char.ReadValueAsync()?.get()?.Value()?;
            let reader = DataReader::FromBuffer(&buffer)?;
            reader
                .ReadByte()
                .map_err(|e| anyhow!("Failed to read byte: {e}"))
        }
        false => Err(anyhow!(
            "Failed to match BLE level UUID:\n{:?}:\n{battery_level_uuid:?}",
            battery_gatt_char.Uuid()?
        )),
    }
}

#[derive(Debug)]
pub enum BluetoothLEDeviceUpdate {
    BatteryLevel(u8),
    ConnectionStatus(bool),
}

pub async fn watch_ble_device(
    ble_device: BluetoothLEDevice,
    exit_flag: &Arc<AtomicBool>,
) -> Result<Option<BluetoothLEDeviceUpdate>> {
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
        .ok_or(anyhow!("Failed to get BLE Battery Gatt Service"))?; // 手机蓝牙无电量服务;

    let battery_gatt_chars = battery_gatt_service
        .GetCharacteristicsForUuidAsync(battery_level_uuid)?
        .get()?
        .Characteristics()
        .map_err(|e| anyhow!("Failed to get BLE Battery Gatt Characteristics: {e}"))?;

    let battery_gatt_char = battery_gatt_chars
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Failed to get BLE Battery Gatt Characteristic"))?;

    if battery_gatt_char.Uuid()? != battery_level_uuid {
        return Err(anyhow!("Battery level characteristic not found"));
    }

    let properties = battery_gatt_char.CharacteristicProperties()?;

    if !properties.contains(GattCharacteristicProperties::Notify) {
        return Err(anyhow!("Battery level does not support notifications"));
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let tx_status = tx.clone();
    let connection_status_token = {
        let handler = TypedEventHandler::new(
            move |sender: windows::core::Ref<BluetoothLEDevice>, _args| {
                if let Some(ble) = sender.as_ref() {
                    let status = ble.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
                    let _ = tx_status.try_send(BluetoothLEDeviceUpdate::ConnectionStatus(status));
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
                    let _ = tx_battery.try_send(BluetoothLEDeviceUpdate::BatteryLevel(battery));
                }
                Ok(())
            },
        );
        battery_gatt_char.ValueChanged(&handler)?
    };

    defer! {
        let _ = ble_device.RemoveConnectionStatusChanged(connection_status_token);
        let _ = battery_gatt_char.RemoveValueChanged(battery_token);
    }

    // let status = battery_gatt_char
    //     .WriteClientCharacteristicConfigurationDescriptorAsync(
    //         GattClientCharacteristicConfigurationDescriptorValue::Notify,
    //     )?
    //     .get()?;
    // if status != GattCommunicationStatus::Success {
    //     // let _ = tx.try_send(BluetoothLEDeviceUpdate::ConnectionStatus(false));
    //     // eprintln!("Failed to subscribe to notifications");
    // }

    tokio::select! {
        maybe_update = rx.recv() => {
            if let Some(update) = maybe_update {
                Ok(Some(update))
            } else {
                Err(anyhow!(
                    "Channel closed while watching BLE Battery: {}",
                    ble_device.Name()?
                ))
            }
        },
        _ = async {
            loop {
                if exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        } => {
            info!("Watch operation was cancelled by exit flag.");
            Ok(None)
        }
    }
}
