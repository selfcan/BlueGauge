use crate::bluetooth::info::BluetoothInfo;
use crate::config::{Config, Direction, TrayIconStyle};
use crate::language::LOC;
use crate::startup::get_startup_status;

use std::collections::HashMap;
use std::ops::Deref;

use anyhow::{Context, Result};
use tray_icon::menu::{
    AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

macro_rules! define_check_menu_items {
    // 提供 enabled（总共4个参数）
    (
        $self:expr,
        [$(($menu_variant:expr, $label:expr, $checked:expr, $enabled:expr)),+ $(,)?]
    ) => {{
        let mut items = Vec::new();
        $(
            let id = $menu_variant.id();
            let item = CheckMenuItem::with_id(id.clone(), $label, $enabled, $checked, None);
            $self.0.insert(id, item.clone());
            items.push(item);
        )+
        items
    }};

    // 未提供 enabled（总共3个参数），菜单允许勾选选项默认为 true
    (
        $self:expr,
        [$(($menu_variant:expr, $label:expr, $checked:expr)),+ $(,)?]
    ) => {{
        define_check_menu_items!($self, [$(($menu_variant, $label, $checked, true)),+])
    }};
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserMenuItem {
    Quit,
    Restart,
    Startup,
    Refresh,
    //
    BluetoothDeviceAddress(u64),
    //
    OpenConfig,
    //
    ShowLowestBatteryDevice,
    //
    SetIconConnectColor,
    //
    TrayIconStyleHorizontalBatteryIcon,
    TrayIconStyleVerticalBatteryIcon,
    TrayIconStyleNumber,
    TrayIconStyleRing,
    //
    TrayTooltipShowDisconnected,
    TrayTooltipTruncateName,
    TrayTooltipPrefixBattery,
    //
    NotifyLowBattery(u8),
    NotifyDeviceChangeDisconnection,
    NotifyDeviceChangeReconnection,
    NotifyDeviceChangeAdded,
    NotifyDeviceChangeRemoved,
    NotifyDeviceStayOnScreen,
}

impl UserMenuItem {
    // 将枚举转换为MenuId
    pub fn id(&mut self) -> MenuId {
        match self {
            UserMenuItem::Quit => MenuId::new("quit"),
            UserMenuItem::Restart => MenuId::new("restart"),
            UserMenuItem::Startup => MenuId::new("startup"),
            UserMenuItem::Refresh => MenuId::new("refresh"),
            //
            UserMenuItem::BluetoothDeviceAddress(u64) => MenuId::from(u64),
            //
            UserMenuItem::OpenConfig => MenuId::new("open_config"),
            //
            UserMenuItem::ShowLowestBatteryDevice => MenuId::new("show_lowest_battery_device"),
            //
            UserMenuItem::SetIconConnectColor => MenuId::new("set_icon_connect_color"),
            //
            UserMenuItem::TrayIconStyleHorizontalBatteryIcon => {
                MenuId::new("horizontal_battery_icon")
            }
            UserMenuItem::TrayIconStyleVerticalBatteryIcon => MenuId::new("vertical_battery_icon"),
            UserMenuItem::TrayIconStyleNumber => MenuId::new("number_icon"),
            UserMenuItem::TrayIconStyleRing => MenuId::new("ring_icon"),
            //
            UserMenuItem::TrayTooltipShowDisconnected => MenuId::new("show_disconnected"),
            UserMenuItem::TrayTooltipTruncateName => MenuId::new("truncate_name"),
            UserMenuItem::TrayTooltipPrefixBattery => MenuId::new("prefix_battery"),
            //
            UserMenuItem::NotifyLowBattery(u8) => MenuId::from(u8),
            UserMenuItem::NotifyDeviceChangeDisconnection => MenuId::new("disconnection"),
            UserMenuItem::NotifyDeviceChangeReconnection => MenuId::new("reconnection"),
            UserMenuItem::NotifyDeviceChangeAdded => MenuId::new("added"),
            UserMenuItem::NotifyDeviceChangeRemoved => MenuId::new("removed"),
            UserMenuItem::NotifyDeviceStayOnScreen => MenuId::new("stay_on_screen"),
        }
    }

    // WARN: 注意数量
    pub fn exclude_bt_address_menu_id() -> Vec<MenuId> {
        let mut include_ids = vec![
            UserMenuItem::Quit.id(),
            UserMenuItem::Restart.id(),
            UserMenuItem::Startup.id(),
            UserMenuItem::Refresh.id(),
            //
            UserMenuItem::OpenConfig.id(),
            //
            UserMenuItem::ShowLowestBatteryDevice.id(),
            //
            UserMenuItem::SetIconConnectColor.id(),
        ];
        include_ids.extend(UserMenuItem::tray_icon_style_menu_id());
        include_ids.extend(UserMenuItem::tray_tooltip_menu_id());
        include_ids.extend(UserMenuItem::low_battery_menu_id());
        include_ids.extend(UserMenuItem::notify_menu_id());
        include_ids
    }

    pub fn low_battery_menu_id() -> [MenuId; 7] {
        [
            UserMenuItem::NotifyLowBattery(0).id(), // 关闭通知
            UserMenuItem::NotifyLowBattery(5).id(),
            UserMenuItem::NotifyLowBattery(10).id(),
            UserMenuItem::NotifyLowBattery(15).id(),
            UserMenuItem::NotifyLowBattery(20).id(),
            UserMenuItem::NotifyLowBattery(25).id(),
            UserMenuItem::NotifyLowBattery(30).id(),
        ]
    }

    pub fn notify_menu_id() -> [MenuId; 5] {
        [
            UserMenuItem::NotifyDeviceChangeDisconnection.id(),
            UserMenuItem::NotifyDeviceChangeReconnection.id(),
            UserMenuItem::NotifyDeviceChangeAdded.id(),
            UserMenuItem::NotifyDeviceChangeRemoved.id(),
            UserMenuItem::NotifyDeviceStayOnScreen.id(),
        ]
    }

    pub fn tray_icon_style_menu_id() -> [MenuId; 4] {
        [
            UserMenuItem::TrayIconStyleHorizontalBatteryIcon.id(),
            UserMenuItem::TrayIconStyleVerticalBatteryIcon.id(),
            UserMenuItem::TrayIconStyleNumber.id(),
            UserMenuItem::TrayIconStyleRing.id(),
        ]
    }

    pub fn tray_tooltip_menu_id() -> [MenuId; 3] {
        [
            UserMenuItem::TrayTooltipShowDisconnected.id(),
            UserMenuItem::TrayTooltipTruncateName.id(),
            UserMenuItem::TrayTooltipPrefixBattery.id(),
        ]
    }
}

struct CreateMenuItem(HashMap<MenuId, CheckMenuItem>);

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

    fn startup(&mut self, text: &str) -> Result<CheckMenuItem> {
        let should_startup = get_startup_status()?;
        let menu_id = UserMenuItem::Startup.id();
        let menu = CheckMenuItem::with_id(menu_id.clone(), text, true, should_startup, None);
        self.0.insert(menu_id, menu.clone());
        Ok(menu)
    }

    fn refresh(text: &str) -> MenuItem {
        MenuItem::with_id(UserMenuItem::Refresh.id(), text, true, None)
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
                let menu_id = UserMenuItem::BluetoothDeviceAddress(info.address).id();
                let menu = CheckMenuItem::with_id(
                    menu_id.clone(),
                    config.get_device_aliases_name(&info.name),
                    true,
                    show_tray_battery_icon_bt_address.is_some_and(|id| id.eq(&info.address)),
                    None,
                );
                self.0.insert(menu_id, menu.clone());
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

        let tray_icon_style_items = define_check_menu_items!(
            self,
            [
                (
                    UserMenuItem::TrayIconStyleHorizontalBatteryIcon,
                    LOC.horizontal_battery_icon,
                    select_horizontal_battery_icon
                ),
                (
                    UserMenuItem::TrayIconStyleVerticalBatteryIcon,
                    LOC.vertical_battery_icon,
                    select_vertical_battery_icon
                ),
                (
                    UserMenuItem::TrayIconStyleNumber,
                    LOC.number_icon,
                    select_number_icon
                ),
                (
                    UserMenuItem::TrayIconStyleRing,
                    LOC.ring_icon,
                    select_ring_icon
                ),
            ]
        );

        let mut menu_tray_icon_style: Vec<&dyn IsMenuItem> = Vec::new();
        menu_tray_icon_style.extend(
            tray_icon_style_items
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        Submenu::with_items(LOC.icon_style, true, &menu_tray_icon_style)
            .expect("Failed to create submenu for tray icon style")
    }

    fn tray_tooltip_options(&mut self, config: &Config) -> Submenu {
        let tray_tooltip_options_items = define_check_menu_items!(
            self,
            [
                (
                    UserMenuItem::TrayTooltipShowDisconnected,
                    LOC.show_disconnected,
                    config.get_show_disconnected()
                ),
                (
                    UserMenuItem::TrayTooltipTruncateName,
                    LOC.truncate_name,
                    config.get_truncate_name()
                ),
                (
                    UserMenuItem::TrayTooltipPrefixBattery,
                    LOC.prefix_battery,
                    config.get_prefix_battery()
                ),
            ]
        );

        let mut menu_tray_tooltip_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_tray_tooltip_options.extend(
            tray_tooltip_options_items
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        Submenu::with_items(LOC.tray_tooltip_options, true, &menu_tray_tooltip_options)
            .expect("Failed to create submenu for tray tooltip options")
    }

    fn notify_low_battery(&mut self, low_battery: u8) -> [CheckMenuItem; 7] {
        UserMenuItem::low_battery_menu_id().map(|menu_id| {
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
            self.0.insert(menu_id, menu.clone());
            menu
        })
    }

    fn notify_device_change(&mut self, config: &Config) -> Vec<CheckMenuItem> {
        define_check_menu_items!(
            self,
            [
                (
                    UserMenuItem::NotifyDeviceChangeDisconnection,
                    LOC.disconnection,
                    config.get_disconnection()
                ),
                (
                    UserMenuItem::NotifyDeviceChangeReconnection,
                    LOC.reconnection,
                    config.get_reconnection()
                ),
                (
                    UserMenuItem::NotifyDeviceChangeAdded,
                    LOC.added,
                    config.get_added()
                ),
                (
                    UserMenuItem::NotifyDeviceChangeRemoved,
                    LOC.removed,
                    config.get_removed()
                ),
            ]
        )
    }

    fn notify_style_config(&mut self, config: &Config) -> Vec<CheckMenuItem> {
        define_check_menu_items!(
            self,
            [(
                UserMenuItem::NotifyDeviceStayOnScreen,
                LOC.stay_on_screen,
                config.get_stay_on_screen()
            ),]
        )
    }

    fn set_icon_connect_color(&mut self, config: &Config) -> CheckMenuItem {
        let menu_id = UserMenuItem::SetIconConnectColor.id();
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

        self.0.insert(menu_id, menu.clone());

        menu
    }

    fn show_lowest_battery_device(&mut self, config: &Config) -> CheckMenuItem {
        let menu_id = UserMenuItem::ShowLowestBatteryDevice.id();
        let menu = CheckMenuItem::with_id(
            menu_id.clone(),
            LOC.show_lowest_battery_device,
            true,
            config.get_show_lowest_battery_device(),
            None,
        );

        self.0.insert(menu_id, menu.clone());

        menu
    }
}

pub fn create_menu(
    config: &Config,
    bluetooth_devices_info: &HashMap<u64, BluetoothInfo>,
) -> Result<(Menu, HashMap<MenuId, CheckMenuItem>)> {
    let mut create_menu_item = CreateMenuItem(HashMap::new());

    let menu_separator = CreateMenuItem::separator();

    let menu_quit = CreateMenuItem::quit(LOC.quit);

    let menu_about = CreateMenuItem::about(LOC.about);

    let menu_refresh = CreateMenuItem::refresh(LOC.refresh);

    let menu_restart = CreateMenuItem::restart(LOC.restart);

    let menu_bluetooth_devicess =
        create_menu_item.bluetooth_devices(config, bluetooth_devices_info)?;
    let menu_bluetooth_devicess: Vec<&dyn IsMenuItem> = menu_bluetooth_devicess
        .iter()
        .map(|item| item as &dyn IsMenuItem)
        .collect();

    let menu_startup = create_menu_item.startup(LOC.startup)?;

    let menu_open_config = &CreateMenuItem::open_config(LOC.open_config);

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

        &Submenu::with_items(LOC.tray_options, true, &menu_tray_options)?
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

        let menu_notify_style_config = create_menu_item.notify_style_config(config);

        let mut menu_notify_options: Vec<&dyn IsMenuItem> = Vec::new();
        menu_notify_options.push(menu_notify_low_battery as &dyn IsMenuItem);
        menu_notify_options.extend(
            menu_notify_device_change
                .iter()
                .map(|item| item as &dyn IsMenuItem),
        );
        menu_notify_options.extend(
            menu_notify_style_config
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

    let tray_menu = Menu::new();
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
