use super::{MenuGroup, MenuKind, MenuManager};
use crate::bluetooth::info::BluetoothInfo;
use crate::config::{Config, Direction, TrayIconStyle};
use crate::language::LOC;
use crate::startup::get_startup_status;

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::LazyLock;

use anyhow::{Context, Result};
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
// GroupSingle
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

/// 定义 CheckMenuItem
/// # 参数
/// - `self`: Stuct `CreateMenu`
/// - `kind`: enum `MenuKind`
/// - `menu_variant`: `MenuId`
/// - `label`: Set `CheckMenu` Text Content
/// - `checked`: Set `CheckMenu` State
/// - `enabled`: Set `CheckMenu` Enable (Default: `true`)
macro_rules! define_check_menu_items {
    // 提供 enabled（总共6个参数）
    (
        $self:expr,
        $kind:expr,
        [$(($menu_variant:expr, $label:expr, $checked:expr, $enabled:expr)),+ $(,)?]
    ) => {{
        let mut menus = Vec::new();
        $(
            let id = $menu_variant.clone();
            let menu = CheckMenuItem::with_id(id.clone(), $label, $enabled, $checked, None);
            $self.0.insert(id, $kind, Some(menu.clone()));
            menus.push(menu);
        )+
        menus
    }};

    // 未提供 enabled（总共5个参数），菜单允许勾选选项默认为 true
    (
        $self:expr,
        $kind:expr,
        [$(($menu_variant:expr, $label:expr, $checked:expr)),+ $(,)?]
    ) => {{
        define_check_menu_items!($self, $kind, [$(($menu_variant, $label, $checked, true)),+])
    }};
}

struct CreateMenuItem(MenuManager);

impl CreateMenuItem {
    fn new() -> Self {
        Self(MenuManager::new())
    }

    fn separator() -> PredefinedMenuItem {
        PredefinedMenuItem::separator()
    }

    fn quit(&mut self, text: &str) -> MenuItem {
        self.0.insert(QUIT.clone(), MenuKind::Normal, None);
        MenuItem::with_id(QUIT.clone(), text, true, None)
    }

    fn about(&mut self, text: &str) -> MenuItem {
        self.0.insert(ABOUT.clone(), MenuKind::Normal, None);
        MenuItem::with_id(ABOUT.clone(), text, true, None)
    }

    fn restart(&mut self, text: &str) -> MenuItem {
        self.0.insert(RESTART.clone(), MenuKind::Normal, None);
        MenuItem::with_id(RESTART.clone(), text, true, None)
    }

    fn open_config(&mut self, text: &str) -> MenuItem {
        self.0.insert(OPEN_CONFIG.clone(), MenuKind::Normal, None);
        MenuItem::with_id(OPEN_CONFIG.clone(), text, true, None)
    }

    fn startup(&mut self, text: &str) -> Result<CheckMenuItem> {
        let should_startup = get_startup_status()?;
        let menu_id = STARTUP.clone();
        let menu = CheckMenuItem::with_id(menu_id.clone(), text, true, should_startup, None);
        self.0
            .insert(STARTUP.clone(), MenuKind::CheckSingle, Some(menu.clone()));
        Ok(menu)
    }

    fn refresh(&mut self, text: &str) -> MenuItem {
        self.0.insert(REFRESH.clone(), MenuKind::Normal, None);
        MenuItem::with_id(REFRESH.clone(), text, true, None)
    }

    fn bluetooth_devices(
        &mut self,
        config: &Config,
        bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
    ) -> Result<Vec<CheckMenuItem>> {
        let show_tray_battery_icon_bt_address = config.get_tray_battery_icon_bt_address();
        let bluetooth_check_items: Vec<CheckMenuItem> = bluetooth_devices_info
            .values()
            .map(|info| {
                let menu_id = MenuId::from(info.address);
                let menu = CheckMenuItem::with_id(
                    menu_id.clone(),
                    config.get_device_aliases_name(&info.name),
                    true,
                    show_tray_battery_icon_bt_address.is_some_and(|id| id.eq(&info.address)),
                    None,
                );
                self.0.insert(
                    menu_id,
                    MenuKind::GroupSingle(MenuGroup::Device, None),
                    Some(menu.clone()),
                );
                menu
            })
            .collect();

        Ok(bluetooth_check_items)
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

        let tray_icon_style_items = define_check_menu_items!(
            self,
            MenuKind::GroupSingle(
                MenuGroup::TrayIconStyle,
                Some(TRAY_ICON_STYLE_NUMBER.clone())
            ),
            [
                (
                    TRAY_ICON_STYLE_HORIZONTAL_BATTERY,
                    LOC.horizontal_battery_icon,
                    select_horizontal_battery_icon
                ),
                (
                    TRAY_ICON_STYLE_VERTICAL_BATTERY,
                    LOC.vertical_battery_icon,
                    select_vertical_battery_icon
                ),
                (TRAY_ICON_STYLE_NUMBER, LOC.number_icon, select_number_icon),
                (TRAY_ICON_STYLE_RING, LOC.ring_icon, select_ring_icon),
                (TRAY_ICON_STYLE_APP, LOC.app_icon, select_app_icon),
            ]
        );

        let menu_tray_icon_style: Vec<&dyn IsMenuItem> = tray_icon_style_items
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();

        Submenu::with_items(LOC.icon_style, true, &menu_tray_icon_style)
            .expect("Failed to create submenu for tray icon style")
    }

    fn tray_tooltip_options(&mut self, config: &Config) -> Submenu {
        let tray_tooltip_options_items = define_check_menu_items!(
            self,
            MenuKind::GroupMulti(MenuGroup::TrayTooltip),
            [
                (
                    TRAY_TOOLTIP_SHOW_DISCONNECTED,
                    LOC.show_disconnected,
                    config.get_show_disconnected()
                ),
                (
                    TRAY_TOOLTIP_TRUNCATE_NAME,
                    LOC.truncate_name,
                    config.get_truncate_name()
                ),
                (
                    TRAY_TOOLTIP_PREFIX_BATTERY,
                    LOC.prefix_battery,
                    config.get_prefix_battery()
                ),
            ]
        );

        let menu_tray_tooltip_options: Vec<&dyn IsMenuItem> = tray_tooltip_options_items
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();

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
            self.0.insert(
                menu_id,
                MenuKind::GroupSingle(MenuGroup::LowBattery, Some(dafault_menu_id)),
                Some(menu.clone()),
            );
            menu
        })
    }

    fn notify_device_change(&mut self, config: &Config) -> Vec<CheckMenuItem> {
        define_check_menu_items!(
            self,
            MenuKind::GroupMulti(MenuGroup::Notify),
            [
                (
                    NOTIFY_DEVICE_CHANGE_DISCONNECTION,
                    LOC.disconnection,
                    config.get_disconnection()
                ),
                (
                    NOTIFY_DEVICE_CHANGE_RECONNECTION,
                    LOC.reconnection,
                    config.get_reconnection()
                ),
                (NOTIFY_DEVICE_CHANGE_ADDED, LOC.added, config.get_added()),
                (
                    NOTIFY_DEVICE_CHANGE_REMOVED,
                    LOC.removed,
                    config.get_removed()
                ),
                (
                    NOTIFY_DEVICE_STAY_ON_SCREEN,
                    LOC.stay_on_screen,
                    config.get_stay_on_screen()
                )
            ]
        )
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
            .insert(menu_id, MenuKind::CheckSingle, Some(menu.clone()));

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
            .insert(menu_id, MenuKind::CheckSingle, Some(menu.clone()));

        menu
    }
}

pub fn create_menu(
    config: &Config,
    bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
) -> Result<(Menu, MenuManager)> {
    let menu_separator = CreateMenuItem::separator();

    let mut create_menu_item = CreateMenuItem::new();

    let menu_about = create_menu_item.about(LOC.about);

    let menu_quit = create_menu_item.quit(LOC.quit);

    let menu_refresh = create_menu_item.refresh(LOC.refresh);

    let menu_restart = create_menu_item.restart(LOC.restart);

    let menu_startup = create_menu_item.startup(LOC.startup)?;

    let menu_open_config = create_menu_item.open_config(LOC.open_config);

    let menu_devices = create_menu_item.bluetooth_devices(config, bluetooth_devices_info)?;
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

    Ok((tray_menu, create_menu_item.0))
}
