use anyhow::{Result, anyhow};
use tokio::task;
use windows::{
    Win32::{
        Devices::DeviceAndDriverInstallation::*, Devices::Properties::*, Foundation::DEVPROPKEY,
    },
    core::*,
};

const DEVPKEY_BLUETOOTH_BATTERY: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5),
    pid: 2,
};

trait CfgRetExt {
    fn to_result(self) -> Result<()>;
}

impl CfgRetExt for CONFIGRET {
    fn to_result(self) -> Result<()> {
        if self == CR_SUCCESS {
            Ok(())
        } else {
            Err(anyhow!("CONFIGRET({})", self.0))
        }
    }
}

async fn poll_battery(instance_id: String, poll_interval: std::time::Duration) {
    loop {
        match read_battery(&instance_id) {
            Ok(level) => {
                println!("[{}] Battery = {}%", instance_id, level);
            }
            Err(err) => {
                println!("[{}] Read error: {:?}", instance_id, err);
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

fn read_battery(instance_id: &str) -> Result<u32> {
    unsafe {
        // Convert instance ID to UTF-16
        let utf16: Vec<u16> = instance_id.encode_utf16().chain([0]).collect();

        // Find devnode
        let mut devnode = 0u32;
        CM_Locate_DevNodeW(
            &mut devnode,
            PWSTR(utf16.as_ptr() as _),
            CM_LOCATE_DEVNODE_NORMAL,
        )
        .to_result()?;

        // Prepare storage
        let mut battery: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let mut prop_type: DEVPROPTYPE = DEVPROP_TYPE_BYTE;

        CM_Get_DevNode_PropertyW(
            devnode,
            &DEVPKEY_BLUETOOTH_BATTERY,
            &mut prop_type,
            Some(&mut battery as *mut _ as *mut u8),
            &mut size,
            0,
        )
        .to_result()?;

        Ok(battery)
    }
}

#[tokio::test]
async fn poll_multiple_bt_classic_devices() -> Result<()> {
    let instance_ids = vec![
        "BTHENUM\\{0000111E-0000-1000-8000-00805F9B34FB}_HCIBYPASS_LOCALMFG&001D\\5&1CD044E&0&B0A3F27C9EA3_C00000000".to_string(),
        // "Device Instance ID".to_string(),
    ];

    let poll_interval = std::time::Duration::from_secs(1);

    for id in instance_ids {
        task::spawn(poll_battery(id, poll_interval));
    }

    tokio::time::sleep(std::time::Duration::from_secs(300)).await;

    Ok(())
}
