use std::collections::HashMap;
use std::ops::Deref;

use crate::bluetooth::info::BluetoothInfo;
use crate::config::{Config, TrayIconStyle};
use crate::language::LOC;
use crate::startup::get_startup_status;

use anyhow::{Context, Result};
use tray_icon::menu::{
    AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserMenuItem {
    Quit,
    Restart,
    Startup,
    //
    BluetoothDeviceAddress(u64),
    //
    OpenConfig,
    //
    TrayIconStyleNumber,
    TrayIconStyleRing,
    SetIconConnectColor,
    //
    TrayTooltipShowDisconnected,
    TrayTooltipTruncateName,
    TrayTooltipPrefixBattery,
    TrayTooltipStayOnScreen,
    //
    NotifyLowBattery(u8),
    NotifyDeviceChangeDisconnection,
    NotifyDeviceChangeReconnection,
    NotifyDeviceChangeAdded,
    NotifyDeviceChangeRemoved,
}

impl UserMenuItem {
    // 将枚举转换为MenuId
    pub fn id(&self) -> MenuId {
        match self {
            UserMenuItem::Quit => MenuId::new("quit"),
            UserMenuItem::Restart => MenuId::new("restart"),
            UserMenuItem::Startup => MenuId::new("startup"),
            //
            UserMenuItem::BluetoothDeviceAddress(u64) => MenuId::from(u64),
            //
            UserMenuItem::OpenConfig => MenuId::new("open_config"),
            //
            UserMenuItem::TrayIconStyleNumber => MenuId::new("number_icon"),
            UserMenuItem::TrayIconStyleRing => MenuId::new("ring_icon"),
            UserMenuItem::SetIconConnectColor => MenuId::new("set_icon_connect_color"),
            //
            UserMenuItem::TrayTooltipShowDisconnected => MenuId::new("show_disconnected"),
            UserMenuItem::TrayTooltipTruncateName => MenuId::new("truncate_name"),
            UserMenuItem::TrayTooltipPrefixBattery => MenuId::new("prefix_battery"),
            UserMenuItem::TrayTooltipStayOnScreen => MenuId::new("stay_on_screen"),
            //
            UserMenuItem::NotifyLowBattery(u8) => MenuId::from(u8),
            UserMenuItem::NotifyDeviceChangeDisconnection => MenuId::new("disconnection"),
            UserMenuItem::NotifyDeviceChangeReconnection => MenuId::new("reconnection"),
            UserMenuItem::NotifyDeviceChangeAdded => MenuId::new("added"),
            UserMenuItem::NotifyDeviceChangeRemoved => MenuId::new("removed"),
        }
    }

    pub fn exclude_bt_address_menu_id() -> [MenuId; 22] {
        [
            UserMenuItem::Quit.id(),
            UserMenuItem::Restart.id(),
            UserMenuItem::Startup.id(),
            //
            UserMenuItem::OpenConfig.id(),
            //
            UserMenuItem::TrayIconStyleNumber.id(),
            UserMenuItem::TrayIconStyleRing.id(),
            UserMenuItem::SetIconConnectColor.id(),
            //
            UserMenuItem::TrayTooltipShowDisconnected.id(),
            UserMenuItem::TrayTooltipTruncateName.id(),
            UserMenuItem::TrayTooltipPrefixBattery.id(),
            UserMenuItem::TrayTooltipStayOnScreen.id(),
            //
            UserMenuItem::NotifyLowBattery(1).id(),
            UserMenuItem::NotifyLowBattery(5).id(),
            UserMenuItem::NotifyLowBattery(10).id(),
            UserMenuItem::NotifyLowBattery(15).id(),
            UserMenuItem::NotifyLowBattery(20).id(),
            UserMenuItem::NotifyLowBattery(25).id(),
            UserMenuItem::NotifyLowBattery(30).id(),
            UserMenuItem::NotifyDeviceChangeDisconnection.id(),
            UserMenuItem::NotifyDeviceChangeReconnection.id(),
            UserMenuItem::NotifyDeviceChangeAdded.id(),
            UserMenuItem::NotifyDeviceChangeRemoved.id(),
        ]
    }

    pub fn low_battery_menu_id() -> [MenuId; 7] {
        [
            UserMenuItem::NotifyLowBattery(1).id(),
            UserMenuItem::NotifyLowBattery(5).id(),
            UserMenuItem::NotifyLowBattery(10).id(),
            UserMenuItem::NotifyLowBattery(15).id(),
            UserMenuItem::NotifyLowBattery(20).id(),
            UserMenuItem::NotifyLowBattery(25).id(),
            UserMenuItem::NotifyLowBattery(30).id(),
        ]
    }

    pub fn notify_menu_id() -> [MenuId; 4] {
        [
            UserMenuItem::NotifyDeviceChangeDisconnection.id(),
            UserMenuItem::NotifyDeviceChangeReconnection.id(),
            UserMenuItem::NotifyDeviceChangeAdded.id(),
            UserMenuItem::NotifyDeviceChangeRemoved.id(),
        ]
    }

    pub fn tray_icon_style_menu_id() -> [MenuId; 2] {
        [
            UserMenuItem::TrayIconStyleNumber.id(),
            UserMenuItem::TrayIconStyleRing.id(),
        ]
    }

    pub fn tray_tooltip_menu_id() -> [MenuId; 4] {
        [
            UserMenuItem::TrayTooltipShowDisconnected.id(),
            UserMenuItem::TrayTooltipTruncateName.id(),
            UserMenuItem::TrayTooltipPrefixBattery.id(),
            UserMenuItem::TrayTooltipStayOnScreen.id(),
        ]
    }
}

struct CreateMenuItem;
impl CreateMenuItem {
    fn separator() -> PredefinedMenuItem {
        PredefinedMenuItem::separator()
    }

    fn quit(text: &str) -> MenuItem {
        MenuItem::with_id(UserMenuItem::Quit.id(), text, true, None)
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
        MenuItem::with_id(UserMenuItem::Restart.id(), text, true, None)
    }

    fn open_config(text: &str) -> MenuItem {
        MenuItem::with_id(UserMenuItem::OpenConfig.id(), text, true, None)
    }

    fn startup(text: &str, tray_check_menus: &mut Vec<CheckMenuItem>) -> Result<CheckMenuItem> {
        let should_startup = get_startup_status()?;
        let menu_startup =
            CheckMenuItem::with_id(UserMenuItem::Startup.id(), text, true, should_startup, None);
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
                    UserMenuItem::BluetoothDeviceAddress(info.address).id(),
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

    fn select_tray_icon_style(
        config: &Config,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> Submenu {
        let tray_icon_style = config.tray_options.tray_icon_style.lock().unwrap().clone();
        let select_number_icon = matches!(tray_icon_style, TrayIconStyle::BatteryNumber { .. });
        let select_ring_icon = matches!(tray_icon_style, TrayIconStyle::BatteryRing { .. });

        let select_tray_icon_style_items = [
            CheckMenuItem::with_id(
                UserMenuItem::TrayIconStyleNumber.id(),
                LOC.number_icon,
                true,
                select_number_icon,
                None,
            ),
            CheckMenuItem::with_id(
                UserMenuItem::TrayIconStyleRing.id(),
                LOC.ring_icon,
                true,
                select_ring_icon,
                None,
            ),
        ];
        tray_check_menus.extend(select_tray_icon_style_items.iter().cloned());

        let mut menu_tray_icon_style: Vec<&dyn IsMenuItem> = Vec::new();
        menu_tray_icon_style.extend(
            select_tray_icon_style_items
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        Submenu::with_items(LOC.icon_style, true, &menu_tray_icon_style)
            .expect("Failed to create submenu for tray icon style")
    }

    #[rustfmt::skip]
    fn set_tray_tooltip(
        config: &Config,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> [CheckMenuItem; 4] {
        let menu_set_tray_tooltip = [
            CheckMenuItem::with_id(UserMenuItem::TrayTooltipShowDisconnected.id(), LOC.show_disconnected, true, config.get_show_disconnected(), None),
            CheckMenuItem::with_id(UserMenuItem::TrayTooltipTruncateName.id(), LOC.truncate_name, true, config.get_truncate_name(), None),
            CheckMenuItem::with_id(UserMenuItem::TrayTooltipPrefixBattery.id(), LOC.prefix_battery, true, config.get_prefix_battery(), None),
            // todo: require localization
            CheckMenuItem::with_id(UserMenuItem::TrayTooltipStayOnScreen.id(), "stay_on_screen", true, config.get_stay_on_screen(), None),
        ];
        tray_check_menus.extend(menu_set_tray_tooltip.iter().cloned());
        menu_set_tray_tooltip
    }

    fn notify_low_battery(
        low_battery: u8,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> [CheckMenuItem; 7] {
        let low_battery_menu_id = UserMenuItem::low_battery_menu_id();
        let menu_low_battery = low_battery_menu_id.map(|menu_id| {
            let id = menu_id.as_ref().parse::<u8>().unwrap();
            CheckMenuItem::with_id(menu_id, format!("{id}%"), true, low_battery == id, None)
        });

        tray_check_menus.extend(menu_low_battery.iter().cloned());
        menu_low_battery
    }

    #[rustfmt::skip]
    fn notify_device_change(
        config: &Config,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> [CheckMenuItem; 4] {
        let menu_device_change = [
            CheckMenuItem::with_id(UserMenuItem::NotifyDeviceChangeDisconnection.id(), LOC.disconnection, true, config.get_disconnection(), None),
            CheckMenuItem::with_id(UserMenuItem::NotifyDeviceChangeReconnection.id(), LOC.reconnection, true, config.get_reconnection(), None),
            CheckMenuItem::with_id(UserMenuItem::NotifyDeviceChangeAdded.id(), LOC.added, true, config.get_added(), None),
            CheckMenuItem::with_id(UserMenuItem::NotifyDeviceChangeRemoved.id(), LOC.removed, true, config.get_removed(), None),
        ];
        tray_check_menus.extend(menu_device_change.iter().cloned());
        menu_device_change
    }

    fn set_icon_connect_color(
        config: &Config,
        tray_check_menus: &mut Vec<CheckMenuItem>,
    ) -> CheckMenuItem {
        let connection_toggle_menu = if let TrayIconStyle::BatteryNumber { color_scheme, .. }
        | TrayIconStyle::BatteryRing { color_scheme, .. } =
            config.tray_options.tray_icon_style.lock().unwrap().deref()
        {
            CheckMenuItem::with_id(
                UserMenuItem::SetIconConnectColor.id(),
                LOC.set_icon_connect_color,
                true,
                color_scheme.is_connect_color(),
                None,
            )
        } else {
            CheckMenuItem::with_id(
                UserMenuItem::SetIconConnectColor.id(),
                LOC.set_icon_connect_color,
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
    let mut tray_check_menus: Vec<CheckMenuItem> = Vec::new();

    let tray_menu = Menu::new();

    let menu_separator = CreateMenuItem::separator();

    let menu_quit = CreateMenuItem::quit(LOC.quit);

    let menu_about = CreateMenuItem::about(LOC.about);

    let menu_restart = CreateMenuItem::restart(LOC.restart);

    let menu_bluetooth_devicess =
        CreateMenuItem::bluetooth_devices(config, &mut tray_check_menus, bluetooth_devices_info)?;
    let menu_bluetooth_devicess: Vec<&dyn IsMenuItem> = menu_bluetooth_devicess
        .iter()
        .map(|item| item as &dyn IsMenuItem)
        .collect();

    let menu_startup = CreateMenuItem::startup(LOC.startup, &mut tray_check_menus)?;

    let menu_open_config = &CreateMenuItem::open_config(LOC.open_config);

    let menu_tray_options = {
        let menu_select_tray_icon_style =
            CreateMenuItem::select_tray_icon_style(config, &mut tray_check_menus);
        let menu_set_icon_connect_color =
            CreateMenuItem::set_icon_connect_color(config, &mut tray_check_menus);
        let menu_set_tray_tooltip = CreateMenuItem::set_tray_tooltip(config, &mut tray_check_menus);

        let mut menu_tray_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_tray_options.push(&menu_select_tray_icon_style as &dyn IsMenuItem);
        menu_tray_options.push(&menu_set_icon_connect_color as &dyn IsMenuItem);
        menu_tray_options.extend(
            menu_set_tray_tooltip
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        &Submenu::with_items(LOC.tray_config, true, &menu_tray_options)?
    };

    let menu_notify_options = {
        let menu_notify_low_battery =
            CreateMenuItem::notify_low_battery(config.get_low_battery(), &mut tray_check_menus);
        let menu_notify_low_battery: Vec<&dyn IsMenuItem> = menu_notify_low_battery
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();
        let menu_notify_low_battery =
            &Submenu::with_items(LOC.low_battery, true, &menu_notify_low_battery)?;

        let menu_notify_device_change =
            CreateMenuItem::notify_device_change(config, &mut tray_check_menus);

        let mut menu_notify_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_notify_options.push(menu_notify_low_battery as &dyn IsMenuItem);
        menu_notify_options.extend(
            menu_notify_device_change
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        &Submenu::with_items(LOC.notify_options, true, &menu_notify_options)?
    };

    let settings_items = &[
        menu_tray_options as &dyn IsMenuItem,
        menu_notify_options as &dyn IsMenuItem,
        menu_open_config as &dyn IsMenuItem,
    ];
    let menu_setting = Submenu::with_items(LOC.settings, true, settings_items)?;

    tray_menu
        .prepend_items(&menu_bluetooth_devicess)
        .context("Failed to prepend 'Bluetooth Items' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_setting)
        .context("Failed to apped 'Setting' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_startup)
        .context("Failed to apped 'Satr up' to Tray Menu")?;
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
