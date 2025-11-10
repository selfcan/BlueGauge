use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use anyhow::{Result, anyhow};
use log::warn;
use piet_common::Color;
use serde::{Deserialize, Serialize};
use tray_icon::menu::MenuId;

use crate::tray::menu_item::UserMenuItem;

pub static EXE_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::current_exe().expect("Failed to get BlueGauge.exe path"));

pub static EXE_PATH_STRING: LazyLock<String> = LazyLock::new(|| {
    EXE_PATH
        .to_str()
        .map(|s| s.to_string())
        .expect("Failed to EXE 'Path' to 'String'")
});

pub static EXE_NAME: LazyLock<String> = LazyLock::new(|| {
    Path::new(&*EXE_PATH)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_owned())
        .expect("Failed to get EXE name")
});

pub static CONFIG_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| EXE_PATH.with_file_name("BlueGauge.toml"));

pub static ASSETS_PATH: LazyLock<PathBuf> = LazyLock::new(|| EXE_PATH.with_file_name("assets"));

macro_rules! impl_atomic_serde {
    ($mod_name:ident, $atomic_type:ty, $inner_type:ty) => {
        mod $mod_name {
            use serde::{Deserialize, Deserializer, Serializer};
            use std::sync::atomic::{Ordering, $atomic_type};

            pub fn serialize<S>(atomic: &$atomic_type, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_some(&atomic.load(Ordering::Relaxed))
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<$atomic_type, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = <$inner_type>::deserialize(deserializer)?;
                Ok(<$atomic_type>::new(value))
            }
        }
    };
}

impl_atomic_serde!(atomic_u8_serde, AtomicU8, u8);
impl_atomic_serde!(atomic_bool_serde, AtomicBool, bool);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "style")]
pub enum TrayIconStyle {
    App,
    BatteryCustom {
        #[serde(rename = "bluetooth_address")]
        address: u64,
    },
    BatteryIcon {
        color_scheme: ColorScheme,
        #[serde(rename = "bluetooth_address")]
        address: u64,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // font_color: Option</* Hex color */ String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        font_size: Option<u8>, // Default: 64
    },
    BatteryNumber {
        color_scheme: ColorScheme,
        #[serde(rename = "bluetooth_address")]
        address: u64,
        font_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        font_color: Option</* Hex color */ String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        font_size: Option<u8>, // Default: 64
    },
    BatteryRing {
        color_scheme: ColorScheme,
        #[serde(rename = "bluetooth_address")]
        address: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        highlight_color: Option</* Hex color */ String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        background_color: Option</* Hex color */ String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ColorScheme {
    ConnectColor, // 连接状态颜色
    Custom,
    #[default]
    FollowSystemTheme, // 跟随系统主题
}

impl ColorScheme {
    pub fn is_connect_color(&self) -> bool {
        matches!(self, ColorScheme::ConnectColor)
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, ColorScheme::Custom)
    }

    pub fn set_custom(&mut self) {
        *self = Self::Custom;
    }

    pub fn set_follow_system_theme(&mut self) {
        *self = Self::FollowSystemTheme;
    }
}

impl TrayIconStyle {
    pub fn update_address(&mut self, new_address: u64) {
        match self {
            Self::App => (),
            Self::BatteryCustom { address }
            | Self::BatteryIcon { address, .. }
            | Self::BatteryNumber { address, .. }
            | Self::BatteryRing { address, .. } => {
                *address = new_address;
            }
        }
    }

    pub fn get_address(&self) -> Option<u64> {
        match self {
            Self::App => None,
            Self::BatteryCustom { address }
            | Self::BatteryIcon { address, .. }
            | Self::BatteryNumber { address, .. }
            | Self::BatteryRing { address, .. } => Some(*address),
        }
    }

    pub fn set_connect_color(&mut self, should_set: bool) {
        match self {
            Self::BatteryNumber { color_scheme, .. }
            | Self::BatteryIcon { color_scheme, .. }
            | Self::BatteryRing { color_scheme, .. } => {
                if should_set {
                    *color_scheme = ColorScheme::ConnectColor;
                } else {
                    *color_scheme = ColorScheme::FollowSystemTheme;
                }
            }
            _ => (),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyOptions {
    #[serde(with = "atomic_u8_serde")]
    pub low_battery: AtomicU8,

    #[serde(with = "atomic_bool_serde")]
    pub disconnection: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub reconnection: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub added: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub removed: AtomicBool,

    #[serde(with = "atomic_bool_serde")]
    pub stay_on_screen: AtomicBool,
}

impl Default for NotifyOptions {
    fn default() -> Self {
        NotifyOptions {
            low_battery: AtomicU8::new(15),
            disconnection: AtomicBool::new(false),
            reconnection: AtomicBool::new(false),
            added: AtomicBool::new(false),
            removed: AtomicBool::new(false),
            stay_on_screen: AtomicBool::new(false),
        }
    }
}

impl NotifyOptions {
    pub fn update(&self, menu_id: &MenuId, check: bool) {
        if menu_id == &UserMenuItem::NotifyDeviceChangeDisconnection.id() {
            self.disconnection.store(check, Ordering::Relaxed)
        }

        if menu_id == &UserMenuItem::NotifyDeviceChangeReconnection.id() {
            self.reconnection.store(check, Ordering::Relaxed)
        }

        if menu_id == &UserMenuItem::NotifyDeviceChangeAdded.id() {
            self.added.store(check, Ordering::Relaxed)
        }

        if menu_id == &UserMenuItem::NotifyDeviceChangeRemoved.id() {
            self.removed.store(check, Ordering::Relaxed)
        }

        if menu_id == &UserMenuItem::NotifyDeviceStayOnScreen.id() {
            self.stay_on_screen.store(check, Ordering::Relaxed)
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct TooltipOptions {
    #[serde(with = "atomic_bool_serde")]
    pub prefix_battery: AtomicBool,
    #[serde(with = "atomic_bool_serde")]
    pub show_disconnected: AtomicBool,
    #[serde(with = "atomic_bool_serde")]
    pub truncate_name: AtomicBool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrayOptions {
    #[serde(rename = "tooltip")]
    pub tooltip_options: TooltipOptions,
    #[serde(rename = "icon")]
    pub tray_icon_style: Mutex<TrayIconStyle>,
}

impl Default for TrayOptions {
    fn default() -> Self {
        TrayOptions {
            tooltip_options: TooltipOptions::default(),
            tray_icon_style: Mutex::new(TrayIconStyle::App),
        }
    }
}

impl TrayOptions {
    pub fn update(&self, menu_id: &MenuId, check: bool) {
        if menu_id == &UserMenuItem::TrayTooltipShowDisconnected.id() {
            self.tooltip_options
                .show_disconnected
                .store(check, Ordering::Relaxed)
        }

        if menu_id == &UserMenuItem::TrayTooltipTruncateName.id() {
            self.tooltip_options
                .truncate_name
                .store(check, Ordering::Relaxed)
        }

        if menu_id == &UserMenuItem::TrayTooltipPrefixBattery.id() {
            self.tooltip_options
                .prefix_battery
                .store(check, Ordering::Relaxed)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "tray")]
    pub tray_options: TrayOptions,
    #[serde(rename = "notify")]
    pub notify_options: NotifyOptions,
    pub device_aliases: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        let device_aliases =
            HashMap::from([("e.g. WH-1000XM6".to_owned(), "Sony Headphones".to_owned())]);

        Self {
            tray_options: TrayOptions::default(),
            notify_options: NotifyOptions::default(),
            device_aliases,
        }
    }
}

impl Config {
    pub fn open() -> Result<Self> {
        let default_config = Config::default();

        Config::read_toml(&CONFIG_PATH).or_else(|e| {
            warn!("Failed to read the config file: {e}\nNow creat a new config file");
            let toml_str = toml::to_string_pretty(&default_config)?;
            std::fs::write(&*CONFIG_PATH, toml_str)?;
            Ok(default_config)
        })
    }

    pub fn save(&self) {
        let toml_str = toml::to_string_pretty(self)
            .expect("Failed to serialize ConfigToml structure as a String of TOML.");
        std::fs::write(&*CONFIG_PATH, toml_str)
            .expect("Failed to write TOML String to BlueGauge.toml");
    }

    fn read_toml(config_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)?;
        let toml_config: Config = toml::from_str(&content)?;

        {
            let mut tray_icon_style = toml_config.tray_options.tray_icon_style.lock().unwrap();

            if find_custom_icon().is_ok() {
                *tray_icon_style = match &*tray_icon_style {
                    TrayIconStyle::App => TrayIconStyle::App,
                    TrayIconStyle::BatteryCustom { address }
                    | TrayIconStyle::BatteryIcon { address, .. }
                    | TrayIconStyle::BatteryNumber { address, .. }
                    | TrayIconStyle::BatteryRing { address, .. } => {
                        TrayIconStyle::BatteryCustom { address: *address }
                    }
                };
            } else {
                match *tray_icon_style {
                    TrayIconStyle::BatteryNumber {
                        ref mut color_scheme,
                        ref font_color,
                        ..
                    } => {
                        if font_color
                            .as_ref()
                            .is_some_and(|c| Color::from_hex_str(c).is_ok())
                        {
                            color_scheme.set_custom();
                        } else if color_scheme.is_custom() {
                            // 如果颜色不存在或错误，且设置自定义，则更改为跟随系统主题
                            color_scheme.set_follow_system_theme();
                        }
                    }
                    TrayIconStyle::BatteryRing {
                        ref mut color_scheme,
                        ref highlight_color,
                        ref background_color,
                        ..
                    } => {
                        let has_valid_custom_color = highlight_color
                            .as_ref()
                            .is_some_and(|c| Color::from_hex_str(c).is_ok())
                            || background_color
                                .as_ref()
                                .is_some_and(|c| Color::from_hex_str(c).is_ok());

                        if has_valid_custom_color {
                            color_scheme.set_custom();
                        } else if color_scheme.is_custom() {
                            // 如果颜色不存在或错误，且设置自定义，则更改为跟随系统主题
                            color_scheme.set_follow_system_theme();
                        }
                    }
                    // TrayIconStyle::BatteryIcon {
                    //     ref mut color_scheme,
                    //     // ref font_color,
                    //     ..
                    // } => {
                    //     // if font_color
                    //     //     .as_ref()
                    //     //     .is_some_and(|c| Color::from_hex_str(c).is_ok())
                    //     // {
                    //     //     color_scheme.set_custom();
                    //     // } else if color_scheme.is_custom() { // 如果颜色不存在或错误，且设置自定义，则更改为跟随系统主题
                    //     //     color_scheme.set_follow_system_theme();
                    //     // }
                    // }
                    _ => (),
                }
            };
        }

        Ok(toml_config)
    }
}

impl Config {
    pub fn get_device_aliases_name(&self, device_name: &String) -> String {
        self.device_aliases
            .get(device_name)
            .unwrap_or(device_name)
            .to_owned()
    }

    pub fn get_stay_on_screen(&self) -> bool {
        self.notify_options.stay_on_screen.load(Ordering::Relaxed)
    }

    pub fn get_prefix_battery(&self) -> bool {
        self.tray_options
            .tooltip_options
            .prefix_battery
            .load(Ordering::Relaxed)
    }

    pub fn get_show_disconnected(&self) -> bool {
        self.tray_options
            .tooltip_options
            .show_disconnected
            .load(Ordering::Relaxed)
    }

    pub fn get_truncate_name(&self) -> bool {
        self.tray_options
            .tooltip_options
            .truncate_name
            .load(Ordering::Relaxed)
    }

    pub fn get_low_battery(&self) -> u8 {
        self.notify_options.low_battery.load(Ordering::Relaxed)
    }

    pub fn get_disconnection(&self) -> bool {
        self.notify_options.disconnection.load(Ordering::Relaxed)
    }

    pub fn get_reconnection(&self) -> bool {
        self.notify_options.reconnection.load(Ordering::Relaxed)
    }

    pub fn get_added(&self) -> bool {
        self.notify_options.added.load(Ordering::Relaxed)
    }

    pub fn get_removed(&self) -> bool {
        self.notify_options.removed.load(Ordering::Relaxed)
    }

    pub fn get_tray_battery_icon_bt_address(&self) -> Option<u64> {
        let tray_icon_style = {
            let lock = self.tray_options.tray_icon_style.lock().unwrap();
            lock.clone()
        };

        match tray_icon_style {
            TrayIconStyle::App => None,
            TrayIconStyle::BatteryCustom { address } => Some(address),
            TrayIconStyle::BatteryIcon { address, .. } => Some(address),
            TrayIconStyle::BatteryNumber { address, .. } => Some(address),
            TrayIconStyle::BatteryRing { address, .. } => Some(address),
        }
    }
}

fn find_custom_icon() -> Result<()> {
    let assets_path = std::env::current_exe().map(|exe_path| exe_path.with_file_name("assets"))?;

    if !assets_path.is_dir() {
        return Err(anyhow!("Assets directory does not exist: {assets_path:?}"));
    }

    let have_custom_default_icons = (0..=100).all(|i| {
        let file_name = format!("{i}.png");
        let file_path = assets_path.join(file_name);
        file_path.is_file()
    });

    if have_custom_default_icons {
        return Ok(());
    }

    let have_custom_theme_icons = (0..=100).all(|i| {
        let file_dark_name = format!("{i}_dark.png");
        let file_light_name = format!("{i}_light.png");
        let file_dark_path = assets_path.join(file_dark_name);
        let file_light_path = assets_path.join(file_light_name);
        file_dark_path.is_file() || file_light_path.is_file()
    });

    if have_custom_theme_icons {
        return Ok(());
    }

    Err(anyhow!(
        "Assets directory does not contain custom battery icons."
    ))
}
