pub mod icon;
pub mod menu;

use super::tray::{
    icon::{load_app_icon, load_tray_icon},
    menu::{MenuManager, item::create_menu},
};
use crate::{
    bluetooth::info::BluetoothInfo,
    config::{Config, TrayIconStyle},
};

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use log::error;
use tray_icon::{TrayIcon, TrayIconBuilder};

#[rustfmt::skip]
pub fn create_tray(
    config: &Config,
    bluetooth_device_map: &DashMap<u64, BluetoothInfo>,
) -> Result<(TrayIcon, MenuManager)> {
    let tray_icon_bt_address = config
        .tray_options
        .tray_icon_style
        .lock()
        .unwrap()
        .get_address();

    let icon = tray_icon_bt_address
        .and_then(|address| bluetooth_device_map.get(&address))
        .map(|info| (info.battery, info.status))
        .and_then(|(battery, status)| {
            load_tray_icon(config, battery, status)
                .inspect_err(|e| error!("Failed to load icon - {e}"))
                .ok()
        })
        .or_else(|| {
            // 载入图标失败时，需更新配置中的图标样式，注意要在创建菜单之前
            *config.tray_options.tray_icon_style.lock().unwrap() = TrayIconStyle::App;
            load_app_icon().ok()
        })
        .expect("Failed to create tray's icon");

    let (tray_menu, tray_check_menus) =
        create_menu(config, bluetooth_device_map).map_err(|e| anyhow!("Failed to create menu. - {e}"))?;

    let bluetooth_tooltip_info = convert_tray_info(bluetooth_device_map, config);

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
    bluetooth_device_map: &DashMap<u64, BluetoothInfo>,
    config: &Config,
) -> Vec<String> {
    let should_truncate_name = config.get_truncate_name();
    let should_prefix_battery = config.get_prefix_battery();
    let should_show_disconnected = config.get_show_disconnected();

    bluetooth_device_map
        .iter()
        .filter_map(|entry| {
            // 根据配置和设备状态决定是否包含在提示中
            let include_in_tooltip = entry.status || should_show_disconnected;

            if include_in_tooltip {
                let name = {
                    let name = config.get_device_aliases_name(&entry.name);
                    truncate_with_ellipsis(should_truncate_name, name, 10)
                };
                let battery = entry.battery;
                let status_icon = if entry.status { "🟢" } else { "🔴" };
                let info = if should_prefix_battery {
                    format!("{status_icon}{battery}% - {name}")
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
        result.push('…');
        result
    } else {
        name.to_string()
    }
}
