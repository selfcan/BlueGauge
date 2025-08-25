use std::{collections::HashSet, ops::Deref, path::Path, sync::atomic::Ordering};
use std::{
    ffi::OsString,
    process::Command,
};

use crate::UserEvent;
use crate::{
    bluetooth::info::BluetoothInfo,
    config::{Config, TrayIconSource},
    notify::app_notify,
    startup::set_startup,
};

use log::error;
use tray_icon::menu::CheckMenuItem;
use winit::event_loop::EventLoopProxy;

pub struct MenuHandlers;

impl MenuHandlers {
    pub fn exit(proxy: EventLoopProxy<UserEvent>) {
        let _ = proxy.send_event(UserEvent::Exit);
    }

    pub fn restart(proxy: EventLoopProxy<UserEvent>) {
        let exe_path = std::env::current_exe().expect("Failed to get path of app");
        let args_os: Vec<OsString> = std::env::args_os().collect();

        let _ = proxy.send_event(UserEvent::Exit);

        if let Err(e) = Command::new(exe_path).args(args_os.iter().skip(1)).spawn() {
            error!("Failed to restart app: {e}");
        }
    }

    pub fn force_update(config: &Config) {
        config.force_update.store(true, Ordering::Relaxed)
    }

    pub fn startup(tray_check_menus: Vec<CheckMenuItem>) {
        if let Some(item) = tray_check_menus.iter().find(|item| item.id() == "startup") {
            set_startup(item.is_checked()).expect("Failed to set Launch at Startup")
        }
    }

    pub fn set_icon_connect_color(
        config: &Config,
        menu_event_id: &str,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        if let Some(item) = tray_check_menus
            .iter()
            .find(|item| item.id().as_ref() == menu_event_id)
        {
            if item.is_checked() {
                config
                    .tray_options
                    .tray_icon_source
                    .lock()
                    .unwrap()
                    .update_connect_color(true);
            } else {
                config
                    .tray_options
                    .tray_icon_source
                    .lock()
                    .unwrap()
                    .update_connect_color(false);
            }

            config.save();
            config.force_update.store(true, Ordering::Relaxed);
        }
    }

    pub fn open_config() {
        let config_path = std::env::current_exe()
            .ok()
            .and_then(|exe_path| exe_path.parent().map(Path::to_path_buf))
            .map(|parent_path| parent_path.join("BlueGauge.toml"))
            .expect("Failed to get config path");
        if let Err(e) = std::process::Command::new("notepad.exe")
            .arg(config_path)
            .spawn()
        {
            app_notify(format!("Failed to open config file - {e}"));
        };
    }

    pub fn set_update_interval(
        config: &Config,
        menu_event_id: &str,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        // 只处理更新蓝牙信息间隔相关的菜单项
        let update_interval_items: Vec<_> = tray_check_menus
            .iter()
            .filter(|item| ["15", "30", "60", "300", "600", "1800"].contains(&item.id().as_ref()))
            .collect();

        // 是否存在被点击且为勾选的项目
        let is_checked = update_interval_items
            .iter()
            .any(|item| item.id().as_ref() == menu_event_id && item.is_checked());

        // 更新所有菜单项状态
        update_interval_items.iter().for_each(|item| {
            let should_check = item.id().as_ref() == menu_event_id && is_checked;
            item.set_checked(should_check);
        });

        // 获取当前勾选的项目对应的电量
        let selected_update_interval = update_interval_items
            .iter()
            .find_map(|item| item.is_checked().then_some(item.id().as_ref()))
            .and_then(|id| id.parse::<u64>().ok());

        // 更新配置
        if let Some(update_interval) = selected_update_interval {
            config
                .tray_options
                .update_interval
                .store(update_interval, Ordering::Relaxed);
        } else {
            let default_update_interval = 60;
            config
                .tray_options
                .update_interval
                .store(default_update_interval, Ordering::Relaxed);

            // 找到并选中默认项
            if let Some(default_item) = update_interval_items
                .iter()
                .find(|i| i.id().as_ref() == default_update_interval.to_string())
            {
                default_item.set_checked(true);
            }
        }

        config.save();
        config.force_update.store(true, Ordering::Relaxed);
    }

    pub fn set_notify_low_battery(
        config: &Config,
        menu_event_id: &str,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        // 只处理低电量阈值相关的菜单项
        let low_battery_items: Vec<_> = tray_check_menus
            .iter()
            .filter(|item| {
                ["0.01", "0.05", "0.1", "0.15", "0.2", "0.25"].contains(&item.id().as_ref())
            })
            .collect();

        // 是否存在被点击且为勾选的项目
        let is_checked = low_battery_items
            .iter()
            .any(|item| item.id().as_ref() == menu_event_id && item.is_checked());

        // 更新所有菜单项状态
        low_battery_items.iter().for_each(|item| {
            let should_check = item.id().as_ref() == menu_event_id && is_checked;
            item.set_checked(should_check);
        });

        // 获取当前勾选的项目对应的电量
        let selected_low_battery = low_battery_items
            .iter()
            .find(|item| item.is_checked())
            .and_then(|item| item.id().as_ref().parse::<f64>().ok());

        // 更新配置
        if let Some(low_battery) = selected_low_battery {
            let low_battery = (low_battery * 100.0).round() as u8;
            config
                .notify_options
                .low_battery
                .store(low_battery, Ordering::Relaxed);
        } else {
            let default_low_battery = 15;
            config
                .notify_options
                .low_battery
                .store(default_low_battery, Ordering::Relaxed);

            // 找到并选中默认项
            if let Some(default_item) = low_battery_items.iter().find(|i| i.id().as_ref() == "0.15")
            {
                default_item.set_checked(true);
            }
        }

        config.save();
    }

    pub fn set_notify_device_change(
        config: &Config,
        menu_event_id: &str,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        // 找到对应的菜单（非子菜单），则更新结构体中的配置及配置文件的内容
        if let Some(item) = tray_check_menus
            .iter()
            .find(|item| item.id().as_ref() == menu_event_id)
        {
            if item.is_checked() {
                config.notify_options.update(menu_event_id, true);
            } else {
                config.notify_options.update(menu_event_id, false);
            }

            config.save();
        }
    }

    pub fn set_tray_tooltip(
        config: &Config,
        menu_event_id: &str,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        if let Some(item) = tray_check_menus
            .iter()
            .find(|item| item.id().as_ref() == menu_event_id)
        {
            if item.is_checked() {
                config.tray_options.update(menu_event_id, true);
                config.save();
            } else {
                config.tray_options.update(menu_event_id, false);
                config.save();
            }
        }

        config.force_update.store(true, Ordering::Relaxed);
    }

    pub fn set_tray_icon_source(
        bluetooth_devices_info: HashSet<BluetoothInfo>,
        config: &Config,
        menu_event_id: &str,
        tray_check_menus: Vec<CheckMenuItem>,
    ) -> Option<BluetoothInfo> {
        let not_bluetooth_item_id = [
            "quit",
            "force_update",
            "startup",
            "open_config",
            "15",
            "30",
            "60",
            "300",
            "600",
            "1800",
            "0.01",
            "0.05",
            "0.1",
            "0.15",
            "0.2",
            "0.25",
            "mute",
            "disconnection",
            "reconnection",
            "added",
            "removed",
            "show_disconnected",
            "truncate_name",
            "prefix_battery",
        ];

        let show_battery_icon_bt_address = menu_event_id.parse::<u64>().expect("Menu Event Id");

        // 只处理显示蓝牙电量图标相关的菜单项
        let bluetooth_menus: Vec<_> = tray_check_menus
            .iter()
            .filter(|item| !not_bluetooth_item_id.contains(&item.id().as_ref()))
            .collect();

        let new_bt_menu_is_checked = bluetooth_menus
            .iter()
            .any(|item| item.id().as_ref() == menu_event_id && item.is_checked());

        bluetooth_menus.iter().for_each(|item| {
            let should_check = item.id().as_ref() == menu_event_id && new_bt_menu_is_checked;
            item.set_checked(should_check);
        });

        let mut original_tray_icon_source = config.tray_options.tray_icon_source.lock().unwrap();

        let need_watch = match original_tray_icon_source.deref() {
            TrayIconSource::App if new_bt_menu_is_checked => {
                let have_custom_icons = std::env::current_exe()
                    .ok()
                    .and_then(|exe_path| exe_path.parent().map(Path::to_path_buf))
                    .map(|p| (0..=100).all(|i| p.join(format!("assets\\{i}.png")).is_file()))
                    .unwrap_or(false);

                if have_custom_icons {
                    *original_tray_icon_source = TrayIconSource::BatteryCustom {
                        address: show_battery_icon_bt_address.to_owned(),
                    };
                } else {
                    *original_tray_icon_source = TrayIconSource::BatteryFont {
                        address: show_battery_icon_bt_address.to_owned(),
                        font_name: "Arial".to_owned(),
                        font_color: Some("FollowSystemTheme".to_owned()),
                        font_size: Some(64),
                    };
                };

                bluetooth_devices_info
                    .iter()
                    .find(|i| i.address == show_battery_icon_bt_address)
                    .cloned()
            }
            TrayIconSource::BatteryCustom { .. } | TrayIconSource::BatteryFont { .. } => {
                if new_bt_menu_is_checked {
                    original_tray_icon_source.update_address(show_battery_icon_bt_address);
                    bluetooth_devices_info
                        .iter()
                        .find(|i| i.address == show_battery_icon_bt_address)
                        .cloned()
                } else {
                    *original_tray_icon_source = TrayIconSource::App;

                    None
                }
            }
            _ => None,
        };

        // 更新配置
        drop(original_tray_icon_source); // 释放锁，避免在Config的svae发生死锁.
        config.save();
        config.force_update.store(true, Ordering::Relaxed);
        need_watch
    }
}
