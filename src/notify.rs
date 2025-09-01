use tauri_winrt_notification::*;

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
    LowBattery(String, u8),
    Added(String),
    Removed(String),
    Reconnect(String),
    Disconnect(String),
}