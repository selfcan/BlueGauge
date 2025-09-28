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
        let battery_offset = 100;
        let battery_zore = 0 + battery_offset;

        match self {
            NotifyEvent::LowBattery(name, battery, address) => {
                match battery_zore + *battery - config.get_low_battery() {
                    num if num <= battery_zore => {
                        if notifyed_devices.lock().unwrap().insert(*address) {
                            let message =
                                format!("{name}: {} {battery}", loc.bluetooth_battery_below);
                            notify(message);
                        }
                    }
                    num if num < 10 + battery_zore => (), // In case, battery level wave
                    _ => {
                        notifyed_devices.lock().unwrap().remove(address);
                    }
                }
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
