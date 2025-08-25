use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ConfigToml {
    #[serde(rename = "tray")]
    tray_options: TrayOptionsToml,

    #[serde(rename = "notify")]
    notify_options: NotifyOptionsToml,

    #[serde(default)]
    #[serde(rename = "device_aliases")]
    device_aliases: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrayOptionsToml {
    update_interval: u64,
    #[serde(rename = "tooltip")]
    tray_tooltip: TrayTooltipToml,
    #[serde(rename = "icon")]
    tray_icon_source: TrayIconSource,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrayTooltipToml {
    show_disconnected: bool,
    truncate_name: bool,
    prefix_battery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", content = "font")]
pub enum TrayIconSource {
    App,
    BatteryCustom {
        address: u64,
    },
    BatteryFont {
        address: u64,
        font_name: String,
        /// "FollowSystemTheme"(Default),
        /// "ConnectColor"(连接状态颜色)
        /// Font Color in hex format (e.g. "#FFFFFF")
        #[serde(skip_serializing_if = "Option::is_none")]
        font_color: Option</* Hex color */ String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        font_size: Option<u8>, // Default: 64
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct NotifyOptionsToml {
    mute: bool,
    low_battery: u8,
    disconnection: bool,
    reconnection: bool,
    added: bool,
    removed: bool,
}

impl TrayIconSource {
    pub fn update_address(&mut self, new_address: u64) {
        match self {
            Self::App => (),
            Self::BatteryCustom { address } => {
                *address = new_address;
            }
            Self::BatteryFont { address, .. } => {
                *address = new_address;
            }
        }
    }

    pub fn get_address(&self) -> Option<u64> {
        match self {
            Self::App => None,
            Self::BatteryCustom { address } => Some(*address),
            Self::BatteryFont { address, .. } => Some(*address),
        }
    }

    pub fn update_connect_color(&mut self, should_update: bool) {
        match self {
            Self::App => (),
            Self::BatteryCustom { address } => {
                if should_update {
                    *self = TrayIconSource::BatteryFont {
                        address: address.to_owned(),
                        font_name: "Arial".to_owned(),
                        font_color: Some("FollowSystemTheme".to_owned()),
                        font_size: Some(64),
                    }
                }
            }
            Self::BatteryFont { font_color, .. } => {
                if should_update {
                    *font_color = Some("ConnectColor".to_owned());
                } else if *font_color == Some("ConnectColor".to_owned()) {
                    *font_color = None;
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct NotifyOptions {
    pub mute: AtomicBool,
    pub low_battery: AtomicU8,
    pub disconnection: AtomicBool,
    pub reconnection: AtomicBool,
    pub added: AtomicBool,
    pub removed: AtomicBool,
}

impl Default for NotifyOptions {
    fn default() -> Self {
        NotifyOptions {
            mute: AtomicBool::new(false),
            low_battery: AtomicU8::new(15),
            disconnection: AtomicBool::new(false),
            reconnection: AtomicBool::new(false),
            added: AtomicBool::new(false),
            removed: AtomicBool::new(false),
        }
    }
}

impl NotifyOptions {
    pub fn update(&self, name: &str, check: bool) {
        match name {
            "mute" => self.mute.store(check, Ordering::Relaxed),
            "disconnection" => self.disconnection.store(check, Ordering::Relaxed),
            "reconnection" => self.reconnection.store(check, Ordering::Relaxed),
            "added" => self.added.store(check, Ordering::Relaxed),
            "removed" => self.removed.store(check, Ordering::Relaxed),
            _ => (),
        }
    }
}

#[derive(Default, Debug)]
pub struct TooltipOptions {
    pub prefix_battery: AtomicBool,
    pub show_disconnected: AtomicBool,
    pub truncate_name: AtomicBool,
}

#[derive(Debug)]
pub struct TrayOptions {
    pub update_interval: AtomicU64,
    pub tooltip_options: TooltipOptions,
    pub tray_icon_source: Mutex<TrayIconSource>,
}

impl Default for TrayOptions {
    fn default() -> Self {
        TrayOptions {
            update_interval: AtomicU64::new(60),
            tooltip_options: TooltipOptions::default(),
            tray_icon_source: Mutex::new(TrayIconSource::App),
        }
    }
}

impl TrayOptions {
    pub fn update(&self, name: &str, check: bool) {
        match name {
            "show_disconnected" => self
                .tooltip_options
                .show_disconnected
                .store(check, Ordering::Relaxed),
            "truncate_name" => self
                .tooltip_options
                .truncate_name
                .store(check, Ordering::Relaxed),
            "prefix_battery" => self
                .tooltip_options
                .prefix_battery
                .store(check, Ordering::Relaxed),
            _ => (),
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub config_path: PathBuf,
    pub force_update: AtomicBool,
    pub tray_options: TrayOptions,
    pub notify_options: NotifyOptions,
    pub device_aliases: HashMap<String, String>,
}

impl Config {
    pub fn open() -> Result<Self> {
        let config_path = env::current_exe()
            .ok()
            .map(|exe_path| exe_path.with_file_name("BlueGauge.toml"))
            .ok_or_else(|| anyhow!("Failed to get config path"))?;

        if config_path.is_file() {
            Config::read_toml(config_path.clone()).or_else(|e| {
                warn!("Failed to read config file: {e}");
                Config::create_toml(config_path)
            })
        } else {
            Config::create_toml(config_path)
        }
    }

    pub fn save(&self) {
        let tray_icon_source = {
            let lock = self.tray_options.tray_icon_source.lock().unwrap();
            lock.clone()
        };
        let toml_config = ConfigToml {
            tray_options: TrayOptionsToml {
                update_interval: self.tray_options.update_interval.load(Ordering::Relaxed),
                tray_tooltip: TrayTooltipToml {
                    show_disconnected: self
                        .tray_options
                        .tooltip_options
                        .show_disconnected
                        .load(Ordering::Relaxed),
                    truncate_name: self
                        .tray_options
                        .tooltip_options
                        .truncate_name
                        .load(Ordering::Relaxed),
                    prefix_battery: self
                        .tray_options
                        .tooltip_options
                        .prefix_battery
                        .load(Ordering::Relaxed),
                },
                tray_icon_source,
            },
            notify_options: NotifyOptionsToml {
                mute: self.notify_options.mute.load(Ordering::Relaxed),
                low_battery: self.notify_options.low_battery.load(Ordering::Relaxed),
                disconnection: self.notify_options.disconnection.load(Ordering::Relaxed),
                reconnection: self.notify_options.reconnection.load(Ordering::Relaxed),
                added: self.notify_options.added.load(Ordering::Relaxed),
                removed: self.notify_options.removed.load(Ordering::Relaxed),
            },
            device_aliases: self.device_aliases.clone(),
        };

        let toml_str = toml::to_string_pretty(&toml_config)
            .expect("Failed to serialize ConfigToml structure as a String of TOML.");
        std::fs::write(&self.config_path, toml_str)
            .expect("Failed to TOML String to BlueGauge.toml");
    }

    fn create_toml(config_path: PathBuf) -> Result<Self> {
        let device_aliases =
            HashMap::from([("e.g. WH-1000XM6".to_owned(), "Sony Headphones".to_owned())]);

        let default_config = ConfigToml {
            tray_options: TrayOptionsToml {
                update_interval: 60,
                tray_tooltip: TrayTooltipToml {
                    show_disconnected: false,
                    truncate_name: false,
                    prefix_battery: false,
                },
                tray_icon_source: TrayIconSource::App,
            },
            notify_options: NotifyOptionsToml {
                mute: false,
                low_battery: 15,
                disconnection: false,
                reconnection: false,
                added: false,
                removed: false,
            },
            device_aliases: device_aliases.clone(),
        };

        let toml_str = toml::to_string_pretty(&default_config)?;
        std::fs::write(&config_path, toml_str)?;

        Ok(Config {
            config_path,
            force_update: AtomicBool::new(false),
            tray_options: TrayOptions {
                update_interval: AtomicU64::new(default_config.tray_options.update_interval),
                tray_icon_source: Mutex::new(default_config.tray_options.tray_icon_source),
                tooltip_options: TooltipOptions {
                    show_disconnected: AtomicBool::new(
                        default_config.tray_options.tray_tooltip.show_disconnected,
                    ),
                    truncate_name: AtomicBool::new(
                        default_config.tray_options.tray_tooltip.truncate_name,
                    ),
                    prefix_battery: AtomicBool::new(
                        default_config.tray_options.tray_tooltip.prefix_battery,
                    ),
                },
            },
            notify_options: NotifyOptions {
                mute: AtomicBool::new(default_config.notify_options.mute),
                low_battery: AtomicU8::new(default_config.notify_options.low_battery),
                disconnection: AtomicBool::new(default_config.notify_options.disconnection),
                reconnection: AtomicBool::new(default_config.notify_options.reconnection),
                added: AtomicBool::new(default_config.notify_options.added),
                removed: AtomicBool::new(default_config.notify_options.removed),
            },
            device_aliases,
        })
    }

    fn read_toml(config_path: PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(&config_path)?;
        let toml_config: ConfigToml = toml::from_str(&content)?;
        let tray_icon_source = if find_custom_icon().is_err() {
            toml_config.tray_options.tray_icon_source
        } else {
            match toml_config.tray_options.tray_icon_source {
                TrayIconSource::App => TrayIconSource::App,
                TrayIconSource::BatteryCustom { address } => {
                    TrayIconSource::BatteryCustom { address }
                }
                TrayIconSource::BatteryFont { address, .. } => {
                    TrayIconSource::BatteryCustom { address }
                }
            }
        };

        Ok(Config {
            config_path,
            force_update: AtomicBool::new(false),
            tray_options: TrayOptions {
                update_interval: AtomicU64::new(toml_config.tray_options.update_interval),
                tray_icon_source: Mutex::new(tray_icon_source),
                tooltip_options: TooltipOptions {
                    show_disconnected: AtomicBool::new(
                        toml_config.tray_options.tray_tooltip.show_disconnected,
                    ),
                    truncate_name: AtomicBool::new(
                        toml_config.tray_options.tray_tooltip.truncate_name,
                    ),
                    prefix_battery: AtomicBool::new(
                        toml_config.tray_options.tray_tooltip.prefix_battery,
                    ),
                },
            },
            notify_options: NotifyOptions {
                mute: AtomicBool::new(toml_config.notify_options.mute),
                low_battery: AtomicU8::new(toml_config.notify_options.low_battery),
                disconnection: AtomicBool::new(toml_config.notify_options.disconnection),
                reconnection: AtomicBool::new(toml_config.notify_options.reconnection),
                added: AtomicBool::new(toml_config.notify_options.added),
                removed: AtomicBool::new(toml_config.notify_options.removed),
            },
            device_aliases: toml_config.device_aliases,
        })
    }
}

impl Config {
    pub fn get_device_aliases_name(&self, device_name: &String) -> String {
        self.device_aliases
            .get(device_name)
            .unwrap_or(device_name)
            .to_owned()
    }

    pub fn get_update_interval(&self) -> u64 {
        self.tray_options.update_interval.load(Ordering::Relaxed)
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

    pub fn get_mute(&self) -> bool {
        self.notify_options.mute.load(Ordering::Relaxed)
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
        let tray_icon_source = {
            let lock = self.tray_options.tray_icon_source.lock().unwrap();
            lock.clone()
        };

        match tray_icon_source {
            TrayIconSource::App => None,
            TrayIconSource::BatteryCustom { address } => Some(address),
            TrayIconSource::BatteryFont { address, .. } => Some(address),
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
