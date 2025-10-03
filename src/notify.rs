use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use tauri_winrt_notification::*;

use crate::{
    config::Config,
    language::{Language, Localization},
};

// HKEY_CLASSES_ROOT\AppUserModelId\Windows.SystemToast.BthQuickPair
const BLUETOOTH_APP_ID: &str = "Windows.SystemToast.BthQuickPair";

pub fn notify(text: impl AsRef<str>) {
    Toast::new(BLUETOOTH_APP_ID)
        .title("BlueGauge")
        .text1(text.as_ref())
        .sound(Some(Sound::Default))
        .duration(Duration::Short)
        .show()
        .expect("Failied to send notification");
}

#[derive(Debug)]
pub enum NotifyEvent {
    LowBattery(String, u8, u64),
    Added(String),
    Removed(String),
    Reconnect(String),
    Disconnect(String),
}

impl NotifyEvent {
    pub fn send(&self, config: &Config, notifyed_devices: Arc<Mutex<HashSet<u64>>>) {
        let language = Language::get_system_language();
        let loc = Localization::get(language);

        match self {
            NotifyEvent::LowBattery(name, battery, address) => {
                let low_threshold = config.get_low_battery() as i32;
                let current_battery = *battery as i32;
                let diff = current_battery - low_threshold;

                if diff <= 0 {
                    if notifyed_devices.lock().unwrap().insert(*address) {
                        let message =
                            format!("{name}: {} {battery}", loc.bluetooth_battery_below);
                        notify(message);
                    }
                } else if diff > 10 {
                    notifyed_devices.lock().unwrap().remove(address);
                }
                // else {
                //   // 电量在 (low_threshold, low_threshold + 10] 范围内：
                //   // 处于“防抖缓冲区”，不通知也不清除，避免反复触发
                // }
            }
            NotifyEvent::Added(name) if config.get_added() => {
                notify(format!("{name}: {}", loc.new_bluetooth_device_add));
            }
            NotifyEvent::Removed(name) if config.get_removed() => {
                notify(format!("{name}: {}", loc.old_bluetooth_device_removed));
            }
            NotifyEvent::Reconnect(name) if config.get_reconnection() => {
                notify(format!("{name}: {}", loc.bluetooth_device_reconnected));
            }
            NotifyEvent::Disconnect(name) if config.get_disconnection() => {
                notify(format!("{name}: {}", loc.bluetooth_device_disconnected));
            }
            _ => (),
        }
    }
}
