use super::MenuGroup;
use crate::bluetooth::info::BluetoothInfo;
use crate::config::{Config, Direction, TrayIconStyle};
use crate::language::LOC;
use crate::startup::get_startup_status;

use std::ops::Deref;
use std::rc::Rc;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use dashmap::DashMap;
use tray_controls::{CheckMenuKind, MenuControl, MenuManager};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

pub static QUIT: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("quit")); // Normal
pub static ABOUT: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("about")); // Normal
pub static RESTART: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("restart")); // Normal
pub static STARTUP: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("startup")); // CheckSingle
pub static REFRESH: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("refresh")); // Normal
// Normal
pub static OPEN_CONFIG: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("open_config"));
// CheckSingle
pub static SHOW_LOWEST_BATTERY_DEVICE: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("show_lowest_battery_device"));
// CheckSingle
pub static SET_ICON_CONNECT_COLOR: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("set_icon_connect_color"));
// GroupSingle
pub static TRAY_ICON_STYLE_APP: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("app_icon"));
pub static TRAY_ICON_STYLE_HORIZONTAL_BATTERY: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("horizontal_battery_icon"));
pub static TRAY_ICON_STYLE_VERTICAL_BATTERY: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("vertical_battery_icon"));
pub static TRAY_ICON_STYLE_NUMBER: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("number_icon"));
pub static TRAY_ICON_STYLE_RING: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("ring_icon"));
// GroupMulti
pub static TRAY_TOOLTIP_SHOW_DISCONNECTED: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("show_disconnected"));
pub static TRAY_TOOLTIP_TRUNCATE_NAME: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("truncate_name"));
pub static TRAY_TOOLTIP_PREFIX_BATTERY: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("prefix_battery"));
// GroupSingle
pub static LOW_BATTERY_0: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(0));
pub static LOW_BATTERY_5: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(5));
pub static LOW_BATTERY_10: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(10));
pub static LOW_BATTERY_15: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(15));
pub static LOW_BATTERY_20: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(20));
pub static LOW_BATTERY_25: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(25));
pub static LOW_BATTERY_30: LazyLock<MenuId> = LazyLock::new(|| MenuId::from(30));
// GroupMulti
pub static NOTIFY_DEVICE_CHANGE_DISCONNECTION: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("disconnection"));
pub static NOTIFY_DEVICE_CHANGE_RECONNECTION: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("reconnection"));
pub static NOTIFY_DEVICE_CHANGE_ADDED: LazyLock<MenuId> = LazyLock::new(|| MenuId::new("added"));
pub static NOTIFY_DEVICE_CHANGE_REMOVED: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("removed"));
pub static NOTIFY_DEVICE_STAY_ON_SCREEN: LazyLock<MenuId> =
    LazyLock::new(|| MenuId::new("stay_on_screen"));

struct CreateMenuItem(MenuManager<MenuGroup>);

impl CreateMenuItem {
    fn new() -> Self {
        Self(MenuManager::new())
    }

    fn separator() -> PredefinedMenuItem {
        PredefinedMenuItem::separator()
    }

    fn quit(&mut self, text: &str) -> MenuItem {
        let menu_item = MenuItem::with_id(QUIT.clone(), text, true, None);
        self.0.insert(MenuControl::MenuItem(menu_item.clone()));
        menu_item
    }

    fn about(&mut self, text: &str) -> MenuItem {
        let menu_item = MenuItem::with_id(ABOUT.clone(), text, true, None);
        self.0.insert(MenuControl::MenuItem(menu_item.clone()));
        menu_item
    }

    fn restart(&mut self, text: &str) -> MenuItem {
        let menu_item = MenuItem::with_id(RESTART.clone(), text, true, None);
        self.0.insert(MenuControl::MenuItem(menu_item.clone()));
        menu_item
    }

    fn open_config(&mut self, text: &str) -> MenuItem {
        let menu_item = MenuItem::with_id(OPEN_CONFIG.clone(), text, true, None);
        self.0.insert(MenuControl::MenuItem(menu_item.clone()));
        menu_item
    }

    fn startup(&mut self, text: &str) -> Result<CheckMenuItem> {
        let should_startup = get_startup_status()?;
        let menu_id = STARTUP.clone();
        let check_menu_item =
            CheckMenuItem::with_id(menu_id.clone(), text, true, should_startup, None);
        self.0
            .insert(MenuControl::CheckMenu(CheckMenuKind::Separate(Rc::new(
                check_menu_item.clone(),
            ))));
        Ok(check_menu_item)
    }

    fn refresh(&mut self, text: &str) -> MenuItem {
        let menu_item = MenuItem::with_id(REFRESH.clone(), text, true, None);
        self.0.insert(MenuControl::MenuItem(menu_item.clone()));
        menu_item
    }

    fn bluetooth_devices(
        &mut self,
        config: &Config,
        bluetooth_devices_info: &DashMap<u64, BluetoothInfo>,
    ) -> Vec<CheckMenuItem> {
        let show_tray_battery_icon_bt_address = config.get_tray_battery_icon_bt_address();

        let mut sorted_devices_info = bluetooth_devices_info
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();

        sorted_devices_info.sort_by(|a, b| {
            // 1. 先按状态排序（🟢在前，🔴在后）
            match (a.status, b.status) {
                (true, false) => std::cmp::Ordering::Less, // true 在 false 前
                (false, true) => std::cmp::Ordering::Greater, // false 在 true 后
                _ => {
                    // 2. 同组内按名称字母顺序排序（A-Z）
                    a.name.cmp(&b.name)
                }
            }
        });

        sorted_devices_info
            .iter()
            .map(|info| {
                let menu_id = MenuId::from(info.address);
                let name = config
                    .get_device_aliases_name(&info.name)
                    .unwrap_or(&info.name);
                let text = format!(
                    "{} - {name} - {}%",
                    if info.status { '♾' } else { '🚫' },
                    info.battery
                );
                let menu = CheckMenuItem::with_id(
                    menu_id.clone(),
                    text,
                    true,
                    show_tray_battery_icon_bt_address.is_some_and(|addr| addr.eq(&info.address)),
                    None,
                );
                self.0.insert(MenuControl::CheckMenu(CheckMenuKind::Radio(
                    Rc::new(menu.clone()),
                    None,
                    MenuGroup::RadioDevice,
                )));
                menu
            })
            .collect::<Vec<CheckMenuItem>>()
    }

    fn tray_icon_style(&mut self, config: &Config) -> Submenu {
        let tray_icon_style = config.tray_options.tray_icon_style.lock().unwrap().clone();

        let select_horizontal_battery_icon = matches!(
            tray_icon_style,
            TrayIconStyle::BatteryIcon {
                direction: Direction::Horizontal,
                ..
            }
        );
        let select_vertical_battery_icon = matches!(
            tray_icon_style,
            TrayIconStyle::BatteryIcon {
                direction: Direction::Vertical,
                ..
            }
        );
        let select_number_icon = matches!(tray_icon_style, TrayIconStyle::BatteryNumber { .. });
        let select_ring_icon = matches!(tray_icon_style, TrayIconStyle::BatteryRing { .. });
        let select_app_icon = matches!(tray_icon_style, TrayIconStyle::App);

        let mut menus = Vec::new();

        [
            (
                TRAY_ICON_STYLE_HORIZONTAL_BATTERY.clone(),
                LOC.horizontal_battery_icon,
                select_horizontal_battery_icon,
            ),
            (
                TRAY_ICON_STYLE_VERTICAL_BATTERY.clone(),
                LOC.vertical_battery_icon,
                select_vertical_battery_icon,
            ),
            (
                TRAY_ICON_STYLE_NUMBER.clone(),
                LOC.number_icon,
                select_number_icon,
            ),
            (
                TRAY_ICON_STYLE_RING.clone(),
                LOC.ring_icon,
                select_ring_icon,
            ),
            (TRAY_ICON_STYLE_APP.clone(), LOC.app_icon, select_app_icon),
        ]
        .into_iter()
        .for_each(|(menu_id, text, checked)| {
            let menu = CheckMenuItem::with_id(menu_id, text, true, checked, None);
            self.0.insert(MenuControl::CheckMenu(CheckMenuKind::Radio(
                Rc::new(menu.clone()),
                Some(Rc::new(TRAY_ICON_STYLE_NUMBER.clone())),
                MenuGroup::RadioTrayIconStyle,
            )));
            menus.push(menu);
        });

        let menu_tray_icon_style: Vec<&dyn IsMenuItem> =
            menus.iter().map(|item| item as &dyn IsMenuItem).collect();

        Submenu::with_items(LOC.icon_style_options, true, &menu_tray_icon_style)
            .expect("Failed to create submenu for tray icon style")
    }

    fn tray_tooltip_options(&mut self, config: &Config) -> Submenu {
        let mut menus = Vec::new();

        [
            (
                TRAY_TOOLTIP_SHOW_DISCONNECTED.clone(),
                LOC.show_disconnected,
                config.get_show_disconnected(),
            ),
            (
                TRAY_TOOLTIP_TRUNCATE_NAME.clone(),
                LOC.truncate_name,
                config.get_truncate_name(),
            ),
            (
                TRAY_TOOLTIP_PREFIX_BATTERY.clone(),
                LOC.prefix_battery,
                config.get_prefix_battery(),
            ),
        ]
        .into_iter()
        .for_each(|(menu_id, text, checked)| {
            let menu = CheckMenuItem::with_id(menu_id, text, true, checked, None);
            self.0
                .insert(MenuControl::CheckMenu(CheckMenuKind::CheckBox(
                    Rc::new(menu.clone()),
                    MenuGroup::CheckBoxTrayTooltip,
                )));
            menus.push(menu);
        });

        let menu_tray_tooltip_options: Vec<&dyn IsMenuItem> =
            menus.iter().map(|item| item as &dyn IsMenuItem).collect();

        Submenu::with_items(LOC.tray_tooltip_options, true, &menu_tray_tooltip_options)
            .expect("Failed to create submenu for tray tooltip options")
    }

    fn notify_low_battery(&mut self, low_battery: u8) -> [CheckMenuItem; 7] {
        [
            LOW_BATTERY_0.clone(),
            LOW_BATTERY_5.clone(),
            LOW_BATTERY_10.clone(),
            LOW_BATTERY_15.clone(),
            LOW_BATTERY_20.clone(),
            LOW_BATTERY_25.clone(),
            LOW_BATTERY_30.clone(),
        ]
        .map(|menu_id| {
            let dafault_menu_id = MenuId::from(low_battery);
            let battery = menu_id.as_ref().parse::<u8>().unwrap();
            let menu = CheckMenuItem::with_id(
                menu_id.clone(),
                if battery.eq(&0) {
                    LOC.never.to_string()
                } else {
                    format!("{battery}%")
                },
                true,
                low_battery == battery,
                None,
            );

            self.0.insert(MenuControl::CheckMenu(CheckMenuKind::Radio(
                Rc::new(menu.clone()),
                Some(Rc::new(dafault_menu_id)),
                MenuGroup::RadioLowBattery,
            )));

            menu
        })
    }

    fn notify_device_change(&mut self, config: &Config) -> Vec<CheckMenuItem> {
        let mut menus = Vec::new();

        [
            (
                NOTIFY_DEVICE_CHANGE_DISCONNECTION.clone(),
                LOC.disconnection,
                config.get_disconnection(),
            ),
            (
                NOTIFY_DEVICE_CHANGE_RECONNECTION.clone(),
                LOC.reconnection,
                config.get_reconnection(),
            ),
            (
                NOTIFY_DEVICE_CHANGE_ADDED.clone(),
                LOC.added,
                config.get_added(),
            ),
            (
                NOTIFY_DEVICE_CHANGE_REMOVED.clone(),
                LOC.removed,
                config.get_removed(),
            ),
            (
                NOTIFY_DEVICE_STAY_ON_SCREEN.clone(),
                LOC.stay_on_screen,
                config.get_stay_on_screen(),
            ),
        ]
        .into_iter()
        .for_each(|(menu_id, text, checked)| {
            let menu = CheckMenuItem::with_id(menu_id, text, true, checked, None);
            self.0
                .insert(MenuControl::CheckMenu(CheckMenuKind::CheckBox(
                    Rc::new(menu.clone()),
                    MenuGroup::CheckBoxNotify,
                )));
            menus.push(menu);
        });

        menus
    }

    fn set_icon_connect_color(&mut self, config: &Config) -> CheckMenuItem {
        let menu_id = SET_ICON_CONNECT_COLOR.clone();
        // 仅 [数字图标]  [圆环图标] [电池图标] 支持连接配色
        let menu = if let TrayIconStyle::BatteryNumber { color_scheme, .. }
        | TrayIconStyle::BatteryRing { color_scheme, .. }
        | TrayIconStyle::BatteryIcon { color_scheme, .. } =
            config.tray_options.tray_icon_style.lock().unwrap().deref()
        {
            CheckMenuItem::with_id(
                menu_id.clone(),
                LOC.set_icon_connect_color,
                true,
                color_scheme.is_connect_color(),
                None,
            )
        } else {
            CheckMenuItem::with_id(
                menu_id.clone(),
                LOC.set_icon_connect_color,
                false,
                false,
                None,
            )
        };

        self.0
            .insert(MenuControl::CheckMenu(CheckMenuKind::Separate(Rc::new(
                menu.clone(),
            ))));

        menu
    }

    fn show_lowest_battery_device(&mut self, config: &Config) -> CheckMenuItem {
        let menu_id = SHOW_LOWEST_BATTERY_DEVICE.clone();
        let menu = CheckMenuItem::with_id(
            menu_id.clone(),
            LOC.show_lowest_battery_device,
            true,
            config.get_show_lowest_battery_device(),
            None,
        );

        self.0
            .insert(MenuControl::CheckMenu(CheckMenuKind::Separate(Rc::new(
                menu.clone(),
            ))));

        menu
    }
}

pub fn create_menu(
    config: &Config,
    bluetooth_devices_info: &DashMap<u64, BluetoothInfo>,
    menu_manager: &mut MenuManager<MenuGroup>,
) -> Result<Menu> {
    let menu_separator = CreateMenuItem::separator();

    let mut create_menu_item = CreateMenuItem::new();

    let menu_about = create_menu_item.about(LOC.about);

    let menu_quit = create_menu_item.quit(LOC.quit);

    let menu_refresh = create_menu_item.refresh(LOC.refresh);

    let menu_restart = create_menu_item.restart(LOC.restart);

    let menu_startup = create_menu_item.startup(LOC.startup)?;

    let menu_open_config = create_menu_item.open_config(LOC.open_config);

    let menu_devices = create_menu_item.bluetooth_devices(config, bluetooth_devices_info);
    let menu_devices: Vec<&dyn IsMenuItem> = menu_devices
        .iter()
        .map(|item| item as &dyn IsMenuItem)
        .collect();

    let menu_tray_options = {
        let menu_show_lowest_battery_device = create_menu_item.show_lowest_battery_device(config);
        let menu_set_icon_connect_color = create_menu_item.set_icon_connect_color(config);
        let menu_tray_icon_style = create_menu_item.tray_icon_style(config);
        let menu_tray_tooltip_options = create_menu_item.tray_tooltip_options(config);

        let menu_tray_options: Vec<&dyn IsMenuItem> = vec![
            &menu_show_lowest_battery_device as &dyn IsMenuItem,
            &menu_set_icon_connect_color as &dyn IsMenuItem,
            &menu_tray_icon_style as &dyn IsMenuItem,
            &menu_tray_tooltip_options as &dyn IsMenuItem,
        ];

        Submenu::with_items(LOC.tray_options, true, &menu_tray_options)?
    };

    let menu_notify_options = {
        let menu_notify_low_battery = create_menu_item.notify_low_battery(config.get_low_battery());
        let menu_notify_low_battery: Vec<&dyn IsMenuItem> = menu_notify_low_battery
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();
        let menu_notify_low_battery =
            &Submenu::with_items(LOC.low_battery, true, &menu_notify_low_battery)?;

        let menu_notify_device_change = create_menu_item.notify_device_change(config);

        let mut menu_notify_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_notify_options.push(menu_notify_low_battery as &dyn IsMenuItem);
        menu_notify_options.extend(
            menu_notify_device_change
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        Submenu::with_items(LOC.notify_options, true, &menu_notify_options)?
    };

    let settings_items = &[
        &menu_tray_options as &dyn IsMenuItem,
        &menu_notify_options as &dyn IsMenuItem,
        &menu_open_config as &dyn IsMenuItem,
    ];
    let menu_setting = Submenu::with_items(LOC.settings, true, settings_items)?;

    *menu_manager = create_menu_item.0;

    let tray_menu = Menu::new();
    tray_menu
        .prepend_items(&menu_devices)
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
        .context("Failed to apped 'Restart' to Tray Menu")?;
    tray_menu
        .append(&menu_separator)
        .context("Failed to apped 'Separator' to Tray Menu")?;
    tray_menu
        .append(&menu_refresh)
        .context("Failed to apped 'Refresh' to Tray Menu")?;
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

    Ok(tray_menu)
}
