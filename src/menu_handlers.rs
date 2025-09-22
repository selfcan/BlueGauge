use std::{ffi::OsString, process::Command};
use std::{ops::Deref, path::Path, sync::atomic::Ordering};

use crate::UserEvent;
use crate::{
    config::{Config, TrayIconSource},
    notify::notify,
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
        let mut args_os: Vec<OsString> = std::env::args_os().collect();

        // 添加重启标志（避免与单实例冲突）
        args_os.push("--restart".into());

        if let Err(e) = Command::new(exe_path).args(args_os.iter().skip(1)).spawn() {
            error!("Failed to restart app: {e}");
        }

        let _ = proxy.send_event(UserEvent::Exit);
    }

    pub fn startup(tray_check_menus: Vec<CheckMenuItem>) {
        if let Some(item) = tray_check_menus.iter().find(|item| item.id() == "startup") {
            set_startup(item.is_checked()).expect("Failed to set Launch at Startup")
        } else {
            error!("Not find startup menu id")
        }
    }

    pub fn set_icon_connect_color(
        config: &Config,
        menu_event_id: &str,
        proxy: EventLoopProxy<UserEvent>,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        if let Some(item) = tray_check_menus
            .iter()
            .find(|item| item.id().as_ref() == menu_event_id)
        {
            config
                .tray_options
                .tray_icon_source
                .lock()
                .unwrap()
                .update_connect_color(item.is_checked());

            config.save();

            let _ = proxy.send_event(UserEvent::UnpdatTray);
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
            notify(format!("Failed to open config file - {e}"));
        };
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
                ["0.01", "0.05", "0.10", "0.15", "0.20", "0.25", "0.30"]
                    .contains(&item.id().as_ref())
            })
            .collect();

        // 是否存在被点击且为勾选的项目
        let is_checked = low_battery_items
            .iter()
            .any(|item| item.id().as_ref() == menu_event_id && item.is_checked());

        // 托盘菜单的其余电量项目设置为未勾选状态
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
        if let Some(item) = tray_check_menus
            .iter()
            .find(|item| item.id().as_ref() == menu_event_id)
        {
            config
                .notify_options
                .update(menu_event_id, item.is_checked());
            config.save();
        }
    }

    pub fn set_tray_tooltip(
        config: &Config,
        menu_event_id: &str,
        proxy: EventLoopProxy<UserEvent>,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        if let Some(item) = tray_check_menus
            .iter()
            .find(|item| item.id().as_ref() == menu_event_id)
        {
            config.tray_options.update(menu_event_id, item.is_checked());
            config.save();
            let _ = proxy.send_event(UserEvent::UnpdatTray);
        }
    }

    pub fn set_battery_icon_style(
        config: &Config,
        menu_event_id: &str,
        proxy: EventLoopProxy<UserEvent>,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        // // 获取托盘中图标样式菜单
        let icon_style_menus: Vec<_> = tray_check_menus
            .iter()
            .filter(|item| ["number_icon", "ring_icon"].contains(&item.id().as_ref()))
            .collect();

        // 有无勾选样式菜单
        let have_new_icon_style_menu_checkd = icon_style_menus
            .iter()
            .any(|item| item.id().as_ref() == menu_event_id && item.is_checked());

        // 托盘菜单的其余样式菜单设置为未勾选状态
        icon_style_menus.iter().for_each(|item| {
            let should_check =
                item.id().as_ref() == menu_event_id && have_new_icon_style_menu_checkd;
            item.set_checked(should_check);
        });

        let mut tray_icon_source = config.tray_options.tray_icon_source.lock().unwrap();

        match menu_event_id {
            "number_icon" if have_new_icon_style_menu_checkd => {
                if let TrayIconSource::BatteryRing { address, .. } = *tray_icon_source {
                    *tray_icon_source = TrayIconSource::BatteryNumber {
                        address,
                        font_name: "Arial".to_owned(),
                        font_color: Some("FollowSystemTheme".to_owned()),
                        font_size: Some(64),
                    }
                }
            }
            "ring_icon" if have_new_icon_style_menu_checkd => {
                if let TrayIconSource::BatteryNumber { address, .. } = *tray_icon_source {
                    *tray_icon_source = TrayIconSource::BatteryRing {
                        address,
                        highlight_color: Some("#4fc478".to_owned()),
                        background_color: Some("#A7A19B".to_owned()),
                    }
                }
            }
            _ => (),
        }

        // 优先释放锁，避免Config执行Svae时发生死锁
        drop(tray_icon_source);
        config.save();

        let _ = proxy.send_event(UserEvent::UnpdatTray);
    }

    // 点击托盘中的设备时的事件
    pub fn handle_device_click(
        config: &Config,
        menu_event_id: &str,
        proxy: EventLoopProxy<UserEvent>,
        tray_check_menus: Vec<CheckMenuItem>,
    ) {
        let not_bluetooth_item_id = [
            "quit",
            "startup",
            "open_config",
            "0.01",
            "0.05",
            "0.10",
            "0.15",
            "0.20",
            "0.25",
            "0.30",
            "disconnection",
            "reconnection",
            "added",
            "removed",
            "show_disconnected",
            "truncate_name",
            "prefix_battery",
            "number_icon",
            "ring_icon",
        ];

        let show_battery_icon_bt_address = menu_event_id.parse::<u64>().expect("Menu Event Id");

        // 获取托盘中蓝牙设备菜单（排除其他设备）
        let bluetooth_menus: Vec<_> = tray_check_menus
            .iter()
            .filter(|item| !not_bluetooth_item_id.contains(&item.id().as_ref()))
            .collect();

        // 有无勾选新设备菜单
        let have_new_device_menu_checkd = bluetooth_menus
            .iter()
            .any(|item| item.id().as_ref() == menu_event_id && item.is_checked());

        // 托盘菜单的其余蓝牙设备设置为未勾选状态
        bluetooth_menus.iter().for_each(|item| {
            let should_check = item.id().as_ref() == menu_event_id && have_new_device_menu_checkd;
            item.set_checked(should_check);
        });

        let mut tray_icon_source = config.tray_options.tray_icon_source.lock().unwrap();

        // · 若原来图标来源为应用图标，且有托盘菜单选择有设备时，根据有无自定义设置相应类型图标
        // · 若原来图标来源指定设备电量图标，如果指定设备取消，则托盘图标变为应用图标，如果为其他设备图标，则更新图标来源中的蓝牙地址
        match tray_icon_source.deref() {
            TrayIconSource::App if have_new_device_menu_checkd => {
                let have_custom_icons = std::env::current_exe()
                    .ok()
                    .and_then(|exe_path| exe_path.parent().map(Path::to_path_buf))
                    .map(|p| {
                        let assets_path = p.join("assets");
                        if assets_path.is_dir() {
                            let light_dir = assets_path.join("light");
                            let dark_dir = assets_path.join("dark");
                            match (light_dir.is_dir(), dark_dir.is_dir()) {
                                (true, true) => [light_dir, dark_dir].into_iter().all(|p| {
                                    (0..=100u32)
                                        .into_iter()
                                        .all(|i| p.join(format!("{i}.png")).is_file())
                                }), // 有主题图标
                                (false, false) => (0..=100u32)
                                    .into_iter()
                                    .all(|i| assets_path.join(format!("{i}.png")).is_file()), //无主题图标
                                _ => false, // 主题图标文件夹缺某一个
                            }
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);

                tray_check_menus
                    .iter()
                    .filter(|item| item.id().as_ref().ends_with("icon"))
                    .for_each(|item| match item.id().as_ref() {
                        "number_icon" if !have_custom_icons => {
                            *tray_icon_source = TrayIconSource::BatteryNumber {
                                address: show_battery_icon_bt_address.to_owned(),
                                font_name: "Arial".to_owned(),
                                font_color: Some("FollowSystemTheme".to_owned()),
                                font_size: Some(64),
                            };

                            item.set_checked(true);
                        }
                        _ => item.set_checked(false),
                    });
            }
            TrayIconSource::BatteryCustom { .. }
            | TrayIconSource::BatteryNumber { .. }
            | TrayIconSource::BatteryRing { .. } => {
                if have_new_device_menu_checkd {
                    tray_icon_source.update_address(show_battery_icon_bt_address);
                } else {
                    *tray_icon_source = TrayIconSource::App;
                }
            }
            _ => (),
        };

        // 优先释放锁，避免Config执行Svae时发生死锁
        drop(tray_icon_source);
        config.save();

        let _ = proxy.send_event(UserEvent::UnpdatTray);
    }
}
