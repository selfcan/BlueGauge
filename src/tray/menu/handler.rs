use super::{MenuGroup, item::*};
use crate::{
    UserEvent,
    config::{CONFIG_PATH, Config, TrayIconStyle},
    startup::set_startup,
};

use std::process::Command;
use std::sync::{Arc, atomic::Ordering};

use anyhow::{Context, Result, anyhow};
use tray_icon::menu::{CheckMenuItem, MenuId};
use winit::event_loop::EventLoopProxy;

pub struct MenuHandler {
    menu_id: MenuId,
    is_normal_menu: bool,
    check_menu: Option<(Option<CheckMenuItem>, Option<MenuGroup>)>,
    config: Arc<Config>,
    proxy: EventLoopProxy<UserEvent>,
}

impl MenuHandler {
    pub fn new(
        menu_id: MenuId,
        is_normal_menu: bool,
        check_menu: Option<(Option<CheckMenuItem>, Option<MenuGroup>)>,
        config: Arc<Config>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            menu_id,
            is_normal_menu,
            check_menu,
            config,
            proxy,
        }
    }

    pub fn run(&self) -> Result<()> {
        let id: &MenuId = &self.menu_id;
        let config = &self.config;
        let proxy = &self.proxy;

        if self.is_normal_menu {
            if id.eq(&*QUIT) {
                proxy
                    .send_event(UserEvent::Exit)
                    .context("Failed to send 'Exit' event")
            } else if id.eq(&*REFRESH) {
                proxy
                    .send_event(UserEvent::Refresh)
                    .context("Failed to send 'Refresh' event")
            } else if id.eq(&*RESTART) {
                proxy
                    .send_event(UserEvent::Restart)
                    .context("Failed to send 'Restart' event")
            } else if id.eq(&*OPEN_CONFIG) {
                Command::new("notepad.exe")
                    .arg(&*CONFIG_PATH)
                    .spawn()
                    .map(|_| ())
                    .context("Failed to open config file")
            } else {
                Err(anyhow!("No match normal menu: {}", id.0))
            }
        } else if let Some((check_menu, group)) = &self.check_menu {
            if let Some(group) = group {
                match group {
                    // GroupSingle
                    MenuGroup::Device => {
                        let mut tray_icon_style =
                            self.config.tray_options.tray_icon_style.lock().unwrap();
                        if let Some(check_menu) = check_menu {
                            let device_menu_id = check_menu.id();
                            // let device_address = device_menu_id.as_ref().parse::<u64>().expect(
                            //     &format!("The menu isn't device menu: {}", device_menu_id.0),
                            // );
                            let device_address =
                                device_menu_id.as_ref().parse::<u64>().unwrap_or_else(|_| {
                                    panic!("The menu isn't device menu: {}", device_menu_id.0)
                                });
                            if matches!(*tray_icon_style, TrayIconStyle::App) {
                                *tray_icon_style =
                                    TrayIconStyle::default_number_icon(device_address);
                            } else {
                                tray_icon_style.update_address(device_address);
                            }
                        } else {
                            // 全部设备未勾选，设置图标样式变回 AppIcon
                            *tray_icon_style = TrayIconStyle::App;
                            config
                                .tray_options
                                .show_lowest_battery_device
                                .store(false, Ordering::Relaxed);
                            let _ = proxy.send_event(UserEvent::UnCheckAboutIconMenu);
                        }

                        drop(tray_icon_style);

                        config.save();

                        proxy
                            .send_event(UserEvent::UpdateIcon)
                            .context("Failed to send 'Update Icon' event")
                    }
                    // GroupMulti
                    MenuGroup::Notify => {
                        let Some(check_menu) = check_menu else {
                            return Err(anyhow!(
                                "The clicked CheckMenu is GroupMulti, but it return GroupSingle(no default): {}",
                                id.0
                            ));
                        };

                        let check_state = check_menu.is_checked();
                        let notify_options = &config.notify_options;
                        let mut have_match = true;

                        if id == &*NOTIFY_DEVICE_CHANGE_DISCONNECTION {
                            notify_options
                                .disconnection
                                .store(check_state, Ordering::Relaxed)
                        } else if id == &*NOTIFY_DEVICE_CHANGE_RECONNECTION {
                            notify_options
                                .reconnection
                                .store(check_state, Ordering::Relaxed)
                        } else if id == &*NOTIFY_DEVICE_CHANGE_ADDED {
                            notify_options.added.store(check_state, Ordering::Relaxed)
                        } else if id == &*NOTIFY_DEVICE_CHANGE_REMOVED {
                            notify_options.removed.store(check_state, Ordering::Relaxed)
                        } else if id == &*NOTIFY_DEVICE_STAY_ON_SCREEN {
                            notify_options
                                .stay_on_screen
                                .store(check_state, Ordering::Relaxed)
                        } else {
                            have_match = false;
                        }

                        if have_match {
                            config.save();
                            Ok(())
                        } else {
                            Err(anyhow!("No match set notify menu: {}", id.0))
                        }
                    }
                    // GroupMulti
                    MenuGroup::TrayTooltip => {
                        let Some(check_menu) = check_menu else {
                            return Err(anyhow!(
                                "The clicked CheckMenu is GroupMulti, but it return GroupSingle(no default: {}",
                                id.0
                            ));
                        };

                        let check_state = check_menu.is_checked();
                        let tooltip_options = &config.tray_options.tooltip_options;
                        let mut have_match = true;

                        if id == &*TRAY_TOOLTIP_SHOW_DISCONNECTED {
                            tooltip_options
                                .show_disconnected
                                .store(check_state, Ordering::Relaxed);
                        } else if id == &*TRAY_TOOLTIP_TRUNCATE_NAME {
                            tooltip_options
                                .truncate_name
                                .store(check_state, Ordering::Relaxed)
                        } else if id == &*TRAY_TOOLTIP_PREFIX_BATTERY {
                            tooltip_options
                                .prefix_battery
                                .store(check_state, Ordering::Relaxed)
                        } else {
                            have_match = false;
                        };

                        if have_match {
                            config.save();
                            proxy
                                .send_event(UserEvent::UpdateTrayTooltip)
                                .context("Failed to send 'Update Tray' event")
                        } else {
                            Err(anyhow!("No match set tray tooltip menu: {}", id.0))
                        }
                    }
                    // GroupSingle
                    MenuGroup::TrayIconStyle => {
                        let Some(check_menu) = check_menu else {
                            return Err(anyhow!(
                                "The clicked CheckMenu is GroupSingle, which have default menu, but it return None: {}",
                                id.0
                            ));
                        };

                        let select_menu_id = check_menu.id();
                        let mut tray_icon_style =
                            config.tray_options.tray_icon_style.lock().unwrap();
                        let mut have_match = true;

                        let Some(address) = tray_icon_style.get_address() else {
                            // 若App图标，即为无勾选设备，则返回
                            return Ok(());
                        };

                        if select_menu_id.eq(&*TRAY_ICON_STYLE_HORIZONTAL_BATTERY) {
                            // 若勾选水平电池图标
                            *tray_icon_style = TrayIconStyle::default_hor_battery_icon(address)
                        } else if select_menu_id.eq(&*TRAY_ICON_STYLE_VERTICAL_BATTERY) {
                            // 若勾选垂直电池图标
                            *tray_icon_style = TrayIconStyle::default_vrt_battery_icon(address)
                        } else if select_menu_id.eq(&*TRAY_ICON_STYLE_NUMBER) {
                            // 若勾选数字图标
                            *tray_icon_style = TrayIconStyle::default_number_icon(address)
                        } else if select_menu_id.eq(&*TRAY_ICON_STYLE_RING) {
                            // 若勾选圆圈图标
                            *tray_icon_style = TrayIconStyle::default_ring_icon(address)
                        } else if select_menu_id.eq(&*TRAY_ICON_STYLE_APP) {
                            // 若勾选圆圈图标
                            *tray_icon_style = TrayIconStyle::App;
                            // 取消勾选所有设备菜单，取消显示最低电量设备选项
                            config
                                .tray_options
                                .show_lowest_battery_device
                                .store(false, Ordering::Relaxed);
                            let _ = proxy.send_event(UserEvent::UnCheckDeviceMenu);
                            let _ = proxy.send_event(UserEvent::UnCheckAboutIconMenu);
                        } else {
                            have_match = false
                        };

                        // 显性释放锁
                        drop(tray_icon_style);

                        if have_match {
                            config.save();
                            proxy
                                .send_event(UserEvent::UpdateIcon)
                                .context("Failed to send 'Update Tray' event")
                        } else {
                            Err(anyhow!("No match set tray icon style menu: {}", id.0))
                        }
                    }
                    // GroupSingle
                    MenuGroup::LowBattery => {
                        let Some(check_menu) = check_menu else {
                            return Err(anyhow!(
                                "The clicked CheckMenu is GroupSingle, which have default menu, but it return None: {}",
                                id.0
                            ));
                        };

                        let low_battery = check_menu.id().as_ref().parse::<u8>()?;
                        let should_notify = low_battery.ne(&0);

                        config.notify_options.low_battery.set_value_and_notify(
                            should_notify.then_some(low_battery),
                            should_notify,
                        );
                        config.save();
                        // 更新托盘是因为某些设备低于
                        proxy
                            .send_event(UserEvent::UpdateIcon)
                            .context("Failed to send 'Update Tray' event")
                    }
                }
            } else {
                // 无分组的 CheckMenu
                let Some(check_menu) = check_menu else {
                    return Err(anyhow!(
                        "The clicked CheckMenu no group, but it return GroupSingle(no default): {}",
                        id.0
                    ));
                };

                if id.eq(&*STARTUP) {
                    set_startup(check_menu.is_checked())
                } else if id.eq(&*SHOW_LOWEST_BATTERY_DEVICE) {
                    config
                        .tray_options
                        .show_lowest_battery_device
                        .store(check_menu.is_checked(), Ordering::Relaxed);

                    config.save();

                    proxy
                        .send_event(UserEvent::UpdateTray)
                        .context("Failed to send 'Update Tray' event")
                } else if id.eq(&*SET_ICON_CONNECT_COLOR) {
                    config
                        .tray_options
                        .tray_icon_style
                        .lock()
                        .unwrap()
                        .set_connect_color(check_menu.is_checked());

                    config.save();

                    self.proxy
                        .send_event(UserEvent::UpdateIcon)
                        .context("Failed to send 'Update Tray' event")
                } else {
                    Err(anyhow!("No match single check menu: {}", id.0))
                }
            }
        } else {
            Err(anyhow!("No match any Menu Handler: {}", id.0))
        }
    }
}
