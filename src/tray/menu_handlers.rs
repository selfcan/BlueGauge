use std::sync::Arc;
use std::{ffi::OsString, process::Command};
use std::{ops::Deref, path::Path, sync::atomic::Ordering};

use super::menu_item::UserMenuItem;

use crate::UserEvent;
use crate::config::ColorScheme;
use crate::{
    config::{Config, TrayIconStyle},
    notify::notify,
    startup::set_startup,
};

use log::error;
use tray_icon::menu::{CheckMenuItem, MenuId};
use winit::event_loop::EventLoopProxy;

pub struct MenuHandlers {
    menu_id: MenuId,
    config: Arc<Config>,
    proxy: EventLoopProxy<UserEvent>,
    tray_check_menus: Vec<CheckMenuItem>,
}

impl MenuHandlers {
    pub fn new(
        menu_id: MenuId,
        config: Arc<Config>,
        proxy: EventLoopProxy<UserEvent>,
        tray_check_menus: Vec<CheckMenuItem>,
    ) -> Self {
        MenuHandlers {
            menu_id,
            config,
            proxy,
            tray_check_menus,
        }
    }

    pub fn run(&self) {
        let menu_id = &self.menu_id;
        let low_battery_menu_id = UserMenuItem::low_battery_menu_id();
        let notify_menu_id = UserMenuItem::notify_menu_id();
        let tray_icon_style_menu_id = UserMenuItem::tray_icon_style_menu_id();
        let tray_tooltip_menu_id = UserMenuItem::tray_tooltip_menu_id();

        if menu_id == &UserMenuItem::Quit.id() {
            self.quit();
        } else if menu_id == &UserMenuItem::Restart.id() {
            self.restart();
        } else if menu_id == &UserMenuItem::Startup.id() {
            self.startup();
        } else if menu_id == &UserMenuItem::OpenConfig.id() {
            self.open_config();
        } else if menu_id == &UserMenuItem::SetIconConnectColor.id() {
            self.set_icon_connect_color();
        } else if low_battery_menu_id.contains(menu_id) {
            self.set_notify_low_battery();
        } else if notify_menu_id.contains(menu_id) {
            self.set_notify_device_change();
        } else if tray_icon_style_menu_id.contains(menu_id) {
            self.set_battery_icon_style();
        } else if tray_tooltip_menu_id.contains(menu_id) {
            self.set_tray_tooltip();
        } else {
            self.handle_device_click();
        }
    }

    fn quit(&self) {
        let _ = self.proxy.send_event(UserEvent::Exit);
    }

    fn restart(&self) {
        let exe_path = std::env::current_exe().expect("Failed to get path of app");
        let mut args_os: Vec<OsString> = std::env::args_os().collect();

        // 添加重启标志（避免与单实例冲突）
        args_os.push("--restart".into());

        if let Err(e) = Command::new(exe_path).args(args_os.iter().skip(1)).spawn() {
            error!("Failed to restart app: {e}");
        }

        let _ = self.proxy.send_event(UserEvent::Exit);
    }

    fn startup(&self) {
        if let Some(item) = self
            .tray_check_menus
            .iter()
            .find(|item| item.id() == "startup")
        {
            set_startup(item.is_checked()).expect("Failed to set Launch at Startup")
        } else {
            error!("Not find startup menu id")
        }
    }

    fn set_icon_connect_color(&self) {
        if let Some(item) = self
            .tray_check_menus
            .iter()
            .find(|item| item.id() == &self.menu_id)
        {
            self.config
                .tray_options
                .tray_icon_style
                .lock()
                .unwrap()
                .set_connect_color(item.is_checked());

            self.config.save();

            let _ = self.proxy.send_event(UserEvent::UnpdatTray);
        }
    }

    fn open_config(&self) {
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

    fn set_notify_low_battery(&self) {
        // 只处理低电量阈值相关的菜单项
        let low_battery_menu_id = UserMenuItem::low_battery_menu_id();
        let low_battery_items: Vec<_> = self
            .tray_check_menus
            .iter()
            .filter(|item| low_battery_menu_id.contains(item.id()))
            .collect();

        // 是否存在被点击且为勾选的项目
        let is_checked = low_battery_items
            .iter()
            .any(|item| item.id() == &self.menu_id && item.is_checked());

        // 托盘菜单的其余电量项目设置为未勾选状态
        low_battery_items.iter().for_each(|item| {
            let should_check = item.id() == &self.menu_id && is_checked;
            item.set_checked(should_check);
        });

        // 获取当前勾选的项目对应的电量
        let selected_low_battery = low_battery_items
            .iter()
            .find(|item| item.is_checked())
            .and_then(|item| item.id().as_ref().parse::<u8>().ok());

        // 更新配置
        if let Some(low_battery) = selected_low_battery {
            self.config
                .notify_options
                .low_battery
                .store(low_battery, Ordering::Relaxed);
        } else {
            let default_low_battery = 15;
            self.config
                .notify_options
                .low_battery
                .store(default_low_battery, Ordering::Relaxed);

            // 找到并选中默认项
            if let Some(default_item) = low_battery_items
                .iter()
                .find(|i| i.id() == &UserMenuItem::NotifyLowBattery(15).id())
            {
                default_item.set_checked(true);
            }
        }

        self.config.save();
    }

    fn set_notify_device_change(&self) {
        if let Some(item) = self
            .tray_check_menus
            .iter()
            .find(|item| item.id() == &self.menu_id)
        {
            self.config
                .notify_options
                .update(&self.menu_id, item.is_checked());
            self.config.save();
        }
    }

    fn set_tray_tooltip(&self) {
        if let Some(item) = self
            .tray_check_menus
            .iter()
            .find(|item| item.id() == &self.menu_id)
        {
            self.config
                .tray_options
                .update(&self.menu_id, item.is_checked());
            self.config.save();
            let _ = self.proxy.send_event(UserEvent::UnpdatTray);
        }
    }

    fn set_battery_icon_style(&self) {
        // // 获取托盘中图标样式菜单
        let icon_style_menus: Vec<_> = self
            .tray_check_menus
            .iter()
            .filter(|item| UserMenuItem::tray_icon_style_menu_id().contains(item.id()))
            .collect();

        // 有无勾选样式菜单
        let have_new_icon_style_menu_checkd = icon_style_menus
            .iter()
            .any(|item| item.id() == &self.menu_id && item.is_checked());

        // 托盘菜单的其余样式菜单设置为未勾选状态
        icon_style_menus.iter().for_each(|item| {
            let should_check = item.id() == &self.menu_id && have_new_icon_style_menu_checkd;
            item.set_checked(should_check);
        });

        let mut tray_icon_style = self.config.tray_options.tray_icon_style.lock().unwrap();

        if have_new_icon_style_menu_checkd {
            if self.menu_id == UserMenuItem::TrayIconStyleNumber.id()
                && let TrayIconStyle::BatteryRing { address, .. } = *tray_icon_style
            {
                *tray_icon_style = TrayIconStyle::BatteryNumber {
                    address,
                    color_scheme: ColorScheme::FollowSystemTheme,
                    font_name: "Arial".to_owned(),
                    font_color: Some(String::new()),
                    font_size: Some(64),
                }
            }

            if self.menu_id == UserMenuItem::TrayIconStyleRing.id()
                && let TrayIconStyle::BatteryNumber { address, .. } = *tray_icon_style
            {
                *tray_icon_style = TrayIconStyle::BatteryRing {
                    address,
                    color_scheme: ColorScheme::FollowSystemTheme,
                    highlight_color: Some(String::new()),
                    background_color: Some(String::new()),
                }
            }
        }

        // 优先释放锁，避免Config执行Svae时发生死锁
        drop(tray_icon_style);
        self.config.save();

        let _ = self.proxy.send_event(UserEvent::UnpdatTray);
    }

    // 点击托盘中的设备时的事件
    fn handle_device_click(&self) {
        let exclude_items_id = UserMenuItem::exclude_bt_address_menu_id();

        let show_battery_icon_bt_address =
            self.menu_id.as_ref().parse::<u64>().expect("Menu Event Id");

        // 获取托盘中蓝牙设备菜单（排除其他设备）
        let bluetooth_menus: Vec<_> = self
            .tray_check_menus
            .iter()
            .filter(|item| !exclude_items_id.contains(item.id()))
            .collect();

        // 有无勾选新设备菜单
        let have_new_device_menu_checkd = bluetooth_menus
            .iter()
            .any(|item| item.id() == &self.menu_id && item.is_checked());

        // 托盘菜单的其余蓝牙设备设置为未勾选状态
        bluetooth_menus.iter().for_each(|item| {
            let should_check = item.id() == &self.menu_id && have_new_device_menu_checkd;
            item.set_checked(should_check);
        });

        let mut tray_icon_style = self.config.tray_options.tray_icon_style.lock().unwrap();

        // · 若原来图标来源为应用图标，且有托盘菜单选择有设备时，根据有无自定义设置相应类型图标
        // · 若原来图标来源指定设备电量图标，如果指定设备取消，则托盘图标变为应用图标，如果为其他设备图标，则更新图标来源中的蓝牙地址
        match tray_icon_style.deref() {
            TrayIconStyle::App if have_new_device_menu_checkd => {
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

                self.tray_check_menus
                    .iter()
                    .filter(|item| UserMenuItem::tray_icon_style_menu_id().contains(item.id()))
                    .for_each(|item| {
                        if have_custom_icons {
                            item.set_checked(false);
                        } else {
                            // 无自定义图标时，有设备被勾选时，首选数字图标
                            if item.id() == &UserMenuItem::TrayIconStyleNumber.id() {
                                *tray_icon_style = TrayIconStyle::BatteryNumber {
                                    address: show_battery_icon_bt_address.to_owned(),
                                    color_scheme: ColorScheme::FollowSystemTheme,
                                    font_name: "Arial".to_owned(),
                                    font_color: Some(String::new()),
                                    font_size: Some(64),
                                };

                                item.set_checked(true);
                            } else {
                                item.set_checked(false);
                            }
                        }
                    });
            }
            TrayIconStyle::BatteryCustom { .. }
            | TrayIconStyle::BatteryNumber { .. }
            | TrayIconStyle::BatteryRing { .. } => {
                if have_new_device_menu_checkd {
                    tray_icon_style.update_address(show_battery_icon_bt_address);
                } else {
                    *tray_icon_style = TrayIconStyle::App;
                }
            }
            _ => (),
        };

        // 优先释放锁，避免Config执行Svae时发生死锁
        drop(tray_icon_style);
        self.config.save();

        let _ = self.proxy.send_event(UserEvent::UnpdatTray);
    }
}
