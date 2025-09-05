use anyhow::{Context, Result, anyhow};
use windows::Devices::Bluetooth::{BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows_pnp::{PnpDevicePropertyValue, PnpEnumerator};
use windows_sys::Win32::{
    Devices::{
        DeviceAndDriverInstallation::GUID_DEVCLASS_SYSTEM,
        Properties::{DEVPKEY_Device_Address, DEVPKEY_Device_FriendlyName},
    },
    Foundation::DEVPROPKEY,
};

#[test]
fn ble() -> Result<()> {
    let ble_aqs_filter = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
    let ble_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&ble_aqs_filter)?
        .GetResults()
        .with_context(|| "Faled to find Bluetooth Low Energy from all devices")?;
    for ble_device_info in ble_devices_info {
        let id = ble_device_info.Id()?;
        let ble = BluetoothLEDevice::FromIdAsync(&id)?.GetResults()?;

        // 961：键盘
        // 962：鼠标
        let appearance = ble.Appearance()?;

        println!(
            "
            {:?}\n{:?}\n{:?}\n{:?}\n{:?}\n",
            ble.Name(),
            ble.BluetoothAddress(),
            ble.DeviceId(),
            id,
            appearance.RawValue()
        );
    }

    Ok(())
}

#[test]
fn btc() -> Result<()> {
    let btc_aqs_filter = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
    let btc_devices_info = DeviceInformation::FindAllAsyncAqsFilter(&btc_aqs_filter)?.GetResults()?;

    fn rfcomm_test(btc: BluetoothDevice) -> Result<()> {
        use windows::Networking::Sockets::StreamSocket;
        use windows::Storage::Streams::DataReader;

        let rfcomm_services = btc.GetRfcommServicesAsync()?.GetResults()?;
        let services = rfcomm_services.Services()?;
        let socket = StreamSocket::new()?;

        for service in services {
            if let Err(e) = socket
                .ConnectAsync(
                    &service.ConnectionHostName()?,
                    &service.ConnectionServiceName()?,
                )?
                .GetResults()
            {
                println!("Err: {e:?}");
                continue;
            };
            let reader = DataReader::CreateDataReader(&socket.InputStream()?)?;
            reader.InputStreamOptions()?;
            reader.LoadAsync(1024)?;
            let output = reader.ReadString(reader.UnconsumedBufferLength()?)?;
            println!("{output:?}")
        }

        Ok(())
    }

    for btc_device_info in btc_devices_info {
        let id = btc_device_info.Id()?;
        let btc = BluetoothDevice::FromIdAsync(&id)?.GetResults()?;
        let status = btc.ConnectionStatus()?.0;

        if status == 0 {
            continue;
        }
        if let Err(e) = rfcomm_test(btc.clone()) {
            println!("{e:?}")
        }

        println!(
            "{:?}\n{:?}\n{:?}\n{:?}\n",
            btc.Name(),
            btc.BluetoothAddress(),
            btc.DeviceId(),
            id,
        );
    }

    Ok(())
}

#[test]
fn pnp() -> Result<()> {
    use windows_sys::Wdk::Devices::Bluetooth::DEVPKEY_Bluetooth_DeviceAddress;
    const BT_INSTANCE_ID: &str = "BTHENUM\\";
    #[allow(non_upper_case_globals)]
    const DEVPKEY_Bluetooth_Battery: DEVPROPKEY = DEVPROPKEY {
        fmtid: windows_sys::core::GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5),
        pid: 2,
    };

    let bt_devices_info =
        PnpEnumerator::enumerate_present_devices_by_device_setup_class(GUID_DEVCLASS_SYSTEM)
            .map_err(|e| anyhow!("Failed to enumerate pnp devices - {e:?}"))?;

    for bt_device_info in bt_devices_info {
        if !bt_device_info.device_instance_id.contains(BT_INSTANCE_ID) {
            continue;
        }

        if let Some(mut props) = bt_device_info.device_instance_properties {
            let name = props
                .remove(&DEVPKEY_Device_FriendlyName.into())
                .and_then(|value| match value {
                    PnpDevicePropertyValue::String(v) => Some(v),
                    _ => None,
                });

            let battery_level = props
                .remove(&DEVPKEY_Bluetooth_Battery.into())
                .and_then(|value| match value {
                    PnpDevicePropertyValue::Byte(v) => Some(v),
                    _ => None,
                });

            let address =
                props
                    .remove(&DEVPKEY_Device_Address.into())
                    .and_then(|value| match value {
                        PnpDevicePropertyValue::String(v) => Some(v),
                        _ => None,
                    });

            let address2 = props
                .remove(&DEVPKEY_Bluetooth_DeviceAddress.into())
                .and_then(|value| match value {
                    PnpDevicePropertyValue::String(v) => Some(v),
                    _ => None,
                });

            // 命令行：Get-WmiObject -Query "select * from win32_PnPEntity" | Where Name -like "HUAWEI FreeBuds Pro Hands-Free AG"

            println!(
                "
                Name: {name:?}
                Battery: {battery_level:?}
                Address-1: {address:?}
                Address-2: {address2:?}
                Instance ID{:?}\n",
                bt_device_info.device_instance_id
            );
        }
    }

    Ok(())
}

// https://github.com/joric/bluetooth-battery-monitor/tree/master/misc
#[test]
fn bt_classic_test() -> Result<()> {
    use windows::{Win32::Devices::Bluetooth::*, Win32::Foundation::*, core::*};

    fn find_devices(h_radio: HANDLE) -> Result<()> {
        let search_params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            hRadio: h_radio,
            fReturnAuthenticated: TRUE,
            fReturnRemembered: TRUE,
            fReturnConnected: TRUE,
            fReturnUnknown: TRUE,
            fIssueInquiry: FALSE,
            cTimeoutMultiplier: 0,
        };

        let mut device_info = BLUETOOTH_DEVICE_INFO {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };

        unsafe {
            let h_find = BluetoothFindFirstDevice(&search_params, &mut device_info)?;
            if h_find.0.cast_const().is_null() {
                return Ok(());
            }

            loop {
                let addr = device_info.Address.Anonymous.rgBytes;
                println!(
                    "Device: {:?}",
                    String::from_utf16_lossy(&device_info.szName)
                );
                println!(
                    "\tAddress: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]
                );
                println!("\tClass: 0x{:08X}", device_info.ulClassofDevice);
                println!("\tConnected: {}", device_info.fConnected.as_bool());
                println!("\tRemembered: {}", device_info.fRemembered.as_bool());
                println!("\tAuthenticated: {}", device_info.fAuthenticated.as_bool());

                if BluetoothFindNextDevice(h_find, &mut device_info).is_err() {
                    break;
                }
            }

            BluetoothFindDeviceClose(h_find)?;
        }

        Ok(())
    }

    fn find_radios() -> Result<()> {
        let radio_params = BLUETOOTH_FIND_RADIO_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
        };

        unsafe {
            let mut h_radio = HANDLE::default();
            let h_find = BluetoothFindFirstRadio(&radio_params, &mut h_radio)?;
            if h_find.0.cast_const().is_null() {
                println!("No radios found.");
                return Ok(());
            }

            loop {
                let mut info = BLUETOOTH_RADIO_INFO {
                    dwSize: std::mem::size_of::<BLUETOOTH_RADIO_INFO>() as u32,
                    ..Default::default()
                };

                BluetoothGetRadioInfo(h_radio, &mut info);
                println!("Radio: {:?}", String::from_utf16_lossy(&info.szName));
                find_devices(h_radio)?;

                if BluetoothFindNextRadio(h_find, &mut h_radio).is_err() {
                    break;
                }
            }

            BluetoothFindRadioClose(h_find)?;
        }

        Ok(())
    }

    find_radios()?;

    Ok(())
}
