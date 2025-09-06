use std::collections::HashMap;
use std::ops::Deref;

use crate::bluetooth::info::BluetoothInfo;
use crate::config::{Config, TrayIconSource};
use crate::icon::{load_app_icon, load_battery_icon};
use crate::language::{Language, Localization};
use crate::startup::get_startup_status;

use anyhow::{Context, Result, anyhow};
use tray_icon::menu::{IsMenuItem, Submenu};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
};

struct CreateMenuItem;
impl CreateMenuItem {
    fn separator() -> PredefinedMenuItem {
        PredefinedMenuItem::separator()
    }

    fn quit(text: &str) -> MenuItem {
        MenuItem::with_id("quit", text, true, None)
    }

    fn about(text: &str) -> PredefinedMenuItem {
        PredefinedMenuItem::about(
            Some(text),
            Some(AboutMetadata {
                name: Some("BlueGauge".to_owned()),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                authors: Some(vec!["iKineticate".to_owned()]),
                website: Some("https://github.com/iKineticate/BlueGauge".to_owned()),
                ..Default::default()
            }),
        )
    }

    fn restart(text: &str) -> MenuItem {
        MenuItem::with_id("restart", text, true, None)
    }

    fn open_config(text: &str) -> MenuItem {
        MenuItem::with_id("open_config", text, true, None)
    }

    fn startup(text: &str, tray_check_menus: &mut Vec<CheckMenuItem>) -> Result<CheckMenuItem> {
        let should_startup = get_startup_status()?;
        let menu_startup = CheckMenuItem::with_id("startup", text, true, should_startup, None);
        tray_check_menus.push(menu_startup.clone());
        Ok(menu_startup)
    }

    fn bluetooth_devices(
        config: &Config,
        tray_check_menus: &mut Vec<CheckMenuItem>,
        bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
    ) -> Result<Vec<CheckMenuItem>> {
        let show_tray_battery_icon_bt_address = config.get_tray_battery_icon_bt_address();
        let bluetooth_check_items: Vec<CheckMenuItem> = bluetooth_devices_info
            .values()
            .map(|info| {
                CheckMenuItem::with_id(
                    info.address,
                    config.get_device_aliases_name(&info.name),
                    true,
                    show_tray_battery_icon_bt_address.is_some_and(|id| id.eq(&info.address)),
                    None,
                )
            })
            .collect();

        tray_check_menus.extend(bluetooth_check_items.iter().cloned());

        Ok(bluetooth_check_items)
    }

    #[rustfmt::skip]
    fn set_tray_tooltip(
        config: &Config,
        loc: &Localization,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> [CheckMenuItem; 3] {
        let menu_set_tray_tooltip = [
            CheckMenuItem::with_id("show_disconnected", loc.show_disconnected, true, config.get_show_disconnected(), None),
            CheckMenuItem::with_id("truncate_name", loc.truncate_name, true, config.get_truncate_name(), None),
            CheckMenuItem::with_id("prefix_battery", loc.prefix_battery, true, config.get_prefix_battery(), None),
        ];
        tray_check_menus.extend(menu_set_tray_tooltip.iter().cloned());
        menu_set_tray_tooltip
    }

    fn notify_low_battery(
        low_battery: u8,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> [CheckMenuItem; 7] {
        let menu_low_battery = [
            CheckMenuItem::with_id("0.01", "1%", true, low_battery == 0, None),
            CheckMenuItem::with_id("0.05", "5%", true, low_battery == 5, None),
            CheckMenuItem::with_id("0.10", "10%", true, low_battery == 10, None),
            CheckMenuItem::with_id("0.15", "15%", true, low_battery == 15, None),
            CheckMenuItem::with_id("0.20", "20%", true, low_battery == 20, None),
            CheckMenuItem::with_id("0.25", "25%", true, low_battery == 25, None),
            CheckMenuItem::with_id("0.30", "30%", true, low_battery == 30, None),
        ];
        tray_check_menus.extend(menu_low_battery.iter().cloned());
        menu_low_battery
    }

    #[rustfmt::skip]
    fn notify_device_change(
        config: &Config,
        loc: &Localization,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> [CheckMenuItem; 4] {
        let menu_device_change = [
            CheckMenuItem::with_id("disconnection", loc.disconnection, true, config.get_disconnection(), None),
            CheckMenuItem::with_id("reconnection", loc.reconnection, true, config.get_reconnection(), None),
            CheckMenuItem::with_id("added", loc.added, true, config.get_added(), None),
            CheckMenuItem::with_id("removed", loc.removed, true, config.get_removed(), None),
        ];
        tray_check_menus.extend(menu_device_change.iter().cloned());
        menu_device_change
    }

    fn set_icon_connect_color(
        config: &Config,
        loc: &Localization,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> CheckMenuItem {
        let connection_toggle_menu = if let TrayIconSource::BatteryFont { font_color, .. } =
            config.tray_options.tray_icon_source.lock().unwrap().deref()
        {
            CheckMenuItem::with_id(
                "set_icon_connect_color",
                loc.set_icon_connect_color,
                true,
                font_color.as_ref().is_some_and(|c| c == "ConnectColor"),
                None,
            )
        } else {
            CheckMenuItem::with_id(
                "set_icon_connect_color",
                loc.set_icon_connect_color,
                false,
                false,
                None,
            )
        };

        tray_check_menus.push(connection_toggle_menu.clone());

        connection_toggle_menu
    }
}

pub fn create_menu(
    config: &Config,
    bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
) -> Result<(Menu, Vec<CheckMenuItem>)> {
    let language = Language::get_system_language();
    let loc = Localization::get(language);

    let mut tray_check_menus: Vec<CheckMenuItem> = Vec::new();

    let tray_menu = Menu::new();

    let menu_separator = CreateMenuItem::separator();

    let menu_quit = CreateMenuItem::quit(loc.quit);

    let menu_about = CreateMenuItem::about(loc.about);

    let menu_restart = CreateMenuItem::restart(loc.restart);

    let menu_bluetooth_devicess =
        CreateMenuItem::bluetooth_devices(config, &mut tray_check_menus, bluetooth_devices_info)?;
    let menu_bluetooth_devicess: Vec<&dyn IsMenuItem> = menu_bluetooth_devicess
        .iter()
        .map(|item| item as &dyn IsMenuItem)
        .collect();

    let menu_startup = &CreateMenuItem::startup(loc.startup, &mut tray_check_menus)?;

    let menu_open_config = &CreateMenuItem::open_config(loc.open_config);

    let menu_tray_options = {
        let menu_set_icon_connect_color =
            CreateMenuItem::set_icon_connect_color(config, loc, &mut tray_check_menus);
        let menu_set_tray_tooltip =
            CreateMenuItem::set_tray_tooltip(config, loc, &mut tray_check_menus);

        let mut menu_tray_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_tray_options.push(&menu_set_icon_connect_color as &dyn IsMenuItem);
        menu_tray_options.extend(
            menu_set_tray_tooltip
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        &Submenu::with_items(loc.tray_config, true, &menu_tray_options)?
    };

    let menu_notify_options = {
        let menu_notify_low_battery =
            CreateMenuItem::notify_low_battery(config.get_low_battery(), &mut tray_check_menus);
        let menu_notify_low_battery: Vec<&dyn IsMenuItem> = menu_notify_low_battery
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();
        let menu_notify_low_battery =
            &Submenu::with_items(loc.low_battery, true, &menu_notify_low_battery)?;

        let menu_notify_device_change =
            CreateMenuItem::notify_device_change(config, loc, &mut tray_check_menus);

        let mut menu_notify_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_notify_options.push(menu_notify_low_battery as &dyn IsMenuItem);
        menu_notify_options.extend(
            menu_notify_device_change
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        &Submenu::with_items(loc.notify_options, true, &menu_notify_options)?
    };

    let settings_items = &[
        menu_tray_options as &dyn IsMenuItem,
        menu_notify_options as &dyn IsMenuItem,
        menu_startup as &dyn IsMenuItem,
        menu_open_config as &dyn IsMenuItem,
    ];
    let menu_setting = Submenu::with_items(loc.settings, true, settings_items)?;

    tray_menu
        .prepend_items(&menu_bluetooth_devicess)
        .context("Failed to prepend 'Bluetooth Items' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_setting)
        .context("Failed to apped 'Settings' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_restart)
        .context("Failed to apped 'Force Update' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_about)
        .context("Failed to apped 'About' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_quit)
        .context("Failed to apped 'Quit' to Tray Menu")?;

    Ok((tray_menu, tray_check_menus))
}

#[rustfmt::skip]
pub fn create_tray(
    config: &Config,
    bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
) -> Result<(TrayIcon, Vec<CheckMenuItem>)> {
    let (tray_menu, tray_check_menus) =
        create_menu(config, bluetooth_devices_info).map_err(|e| anyhow!("Failed to create menu. - {e}"))?;

    let tray_icon_bt_address = {
        config
            .tray_options
            .tray_icon_source
            .lock()
            .unwrap()
            .get_address()
    };

    let icon = tray_icon_bt_address
        .and_then(|address| bluetooth_devices_info.get(&address))
        .map(|info| (info.battery, info.status))
        .and_then(|(battery, status)| load_battery_icon(config, battery, status).ok())
        .or_else(|| load_app_icon().ok())
        .expect("Failed to create tray's icon");

    let bluetooth_tooltip_info = convert_tray_info(bluetooth_devices_info, config);

    let tray_icon = TrayIconBuilder::new()
        .with_menu_on_left_click(true)
        .with_icon(icon)
        .with_tooltip(bluetooth_tooltip_info.join("\n"))
        .with_menu(Box::new(tray_menu))
        .build()
        .map_err(|e| anyhow!("Failed to build tray - {e}"))?;

    Ok((tray_icon, tray_check_menus))
}

/// 返回托盘提示及菜单内容
pub fn convert_tray_info(
    bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
    config: &Config,
) -> Vec<String> {
    let should_truncate_name = config.get_truncate_name();
    let should_prefix_battery = config.get_prefix_battery();
    let should_show_disconnected = config.get_show_disconnected();

    bluetooth_devices_info
        .iter()
        .filter_map(|(_, info)| {
            // 根据配置和设备状态决定是否包含在提示中
            let include_in_tooltip = info.status || should_show_disconnected;

            if include_in_tooltip {
                let name = {
                    let name = config.get_device_aliases_name(&info.name);
                    truncate_with_ellipsis(should_truncate_name, name, 10)
                };
                let battery = info.battery;
                let status_icon = if info.status { "🟢" } else { "🔴" };
                let info = if should_prefix_battery {
                    format!("{status_icon}{battery:3}% - {name}")
                } else {
                    format!("{status_icon}{name} - {battery}%")
                };
                Some(info)
            } else {
                None
            }
        })
        .collect()
}

fn truncate_with_ellipsis(truncate_device_name: bool, name: String, max_chars: usize) -> String {
    if truncate_device_name && name.chars().count() > max_chars {
        let mut result = name.chars().take(max_chars).collect::<String>();
        result.push_str("...");
        result
    } else {
        name.to_string()
    }
}
