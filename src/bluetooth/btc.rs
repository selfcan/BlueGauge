use crate::bluetooth::info::{BluetoothInfo, BluetoothType};

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use log::{error, info, warn};
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

#[allow(non_upper_case_globals)]
const DEVPKEY_Bluetooth_Battery: DEVPROPKEY = DEVPROPKEY {
    fmtid: windows_sys::core::GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5),
    pid: 2,
};
const BT_INSTANCE_ID: &str = "BTHENUM\\";

pub struct PnpDeviceInfo {
    pub address: u64,
    pub battery: u8,
    pub instance_id: String,
}

pub fn find_btc_devices() -> Result<Vec<BluetoothDevice>> {
    let btc_aqs_filter = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;

    let btc_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&btc_aqs_filter)?
        .get()
        .with_context(|| "Faled to find Bluetooth Classic from all devices")?;

    let btc_devices = btc_devices_info
        .into_iter()
        .filter_map(|device_info| {
            BluetoothDevice::FromIdAsync(&device_info.Id().ok()?)
                .ok()?
                .get()
                .ok()
        })
        .collect::<Vec<_>>();

    Ok(btc_devices)
}

pub fn get_btc_device_from_address(address: u64) -> Result<BluetoothDevice> {
    BluetoothDevice::FromBluetoothAddressAsync(address)?
        .get()
        .map_err(|e| anyhow!("Failed to find btc ({address}) - {e}"))
}

pub fn get_btc_devices_info(
    btc_devices: &[BluetoothDevice],
) -> Result<HashMap<u64, BluetoothInfo>> {
    // 获取Pnp设备可能出错（初始化可能失败），需重试多次避开错误
    let pnp_devices_info = {
        let max_retries = 2;
        let mut attempts = 0;

        loop {
            let pnp_devices = get_pnp_devices()?;
            match get_pnp_devices_info(pnp_devices) {
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
                    std::thread::sleep(std::time::Duration::from_secs(2));
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

pub fn process_btc_device(
    btc_device: &BluetoothDevice,
    pnp_devices_info: &HashMap<u64, PnpDeviceInfo>,
) -> Result<BluetoothInfo> {
    let btc_name = btc_device.Name()?.to_string().trim().to_owned();

    let btc_address = btc_device.BluetoothAddress()?;

    let (pnp_instance_id, btc_battery) = pnp_devices_info
        .get(&btc_address)
        .map(|i| (i.instance_id.clone(), i.battery))
        .ok_or_else(|| anyhow!("No matching Bluetooth Classic Device in Pnp device: {btc_name}"))?;

    let btc_status = btc_device.ConnectionStatus()? == BluetoothConnectionStatus::Connected;

    Ok(BluetoothInfo {
        name: btc_name,
        battery: btc_battery,
        status: btc_status,
        address: btc_address,
        r#type: BluetoothType::Classic(pnp_instance_id),
    })
}

pub fn get_btc_info_device_frome_address(name: String, address: u64, status: bool) -> Result<BluetoothInfo> {
    let btc_address_bytes = format!("{address:012X}");

    let pnp_devices_node_info = PnpEnumerator::enumerate_present_devices_and_filter_by_device_setup_class(
        GUID_DEVCLASS_SYSTEM,
        PnpFilter::Contains(&[BT_INSTANCE_ID.to_owned(), btc_address_bytes]),
    )
    .map_err(|e| anyhow!("Failed to enumerate pnp device ({address}) - {e:?}"))?;

    let pnp_device_info = get_pnp_devices_info(pnp_devices_node_info)?
        .remove(&address)
        .ok_or_else(|| anyhow!("Failed to obtain the corresponding PNP device from the BTC address"))?;

    Ok(BluetoothInfo {
        name,
        battery: pnp_device_info.battery,
        status,
        address,
        r#type: BluetoothType::Classic(pnp_device_info.instance_id),
    })
}

pub fn get_pnp_devices() -> Result<Vec<PnpDeviceNodeInfo>> {
    PnpEnumerator::enumerate_present_devices_and_filter_by_device_setup_class(
        GUID_DEVCLASS_SYSTEM,
        PnpFilter::Contains(&[BT_INSTANCE_ID.to_owned()]),
    )
    .map_err(|e| anyhow!("Failed to enumerate pnp devices - {e:?}"))
}

pub fn get_pnp_devices_info(pnp_devices_node_info: Vec<PnpDeviceNodeInfo>) -> Result<HashMap<u64, PnpDeviceInfo>> {
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
                        address,
                        battery,
                        instance_id: pnp_device_node_info.device_instance_id,
                    },
                );
            }
        }
    }

    Ok(pnp_devices_info)
}

pub async fn watch_btc_devices_status_async(
    btc_devices: Vec<BluetoothDevice>,
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicBool>,
) -> Result<Option<(u64, bool)>> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let mut guard = scopeguard::guard(Vec::<(BluetoothDevice, _)>::new(), |v| {
        for (device, connection_status_token) in v {
            let _ = device.RemoveConnectionStatusChanged(connection_status_token);
        }
    });

    for btc_device in btc_devices {
        let address = btc_device.BluetoothAddress()?;

        let tx_status = tx.clone();
        let connection_status_token = {
            let handler = TypedEventHandler::new(
                move |sender: windows::core::Ref<BluetoothDevice>, _args| {
                    if let Some(btc) = sender.as_ref() {
                        let status =
                            btc.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
                        let _ = tx_status.try_send((address, status));
                    }
                    Ok(())
                },
            );
            btc_device.ConnectionStatusChanged(&handler)?
        };

        guard.push((btc_device, connection_status_token));
    }

    tokio::select! {
        maybe_update = rx.recv() => {
            if let Some(update) = maybe_update {
                Ok(Some(update))
            } else {
                Err(anyhow!("Channel closed while watching BLE devcies"))
            }
        },
        _ = async {
            loop {
                if exit_flag.load(Ordering::Relaxed)
                    || restart_flag.swap(false, Ordering::Relaxed)
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        } => {
            info!("Watch BTC Status was cancelled by exit flag.");
            Ok(None)
        }
    }
}
