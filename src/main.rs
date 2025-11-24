#![allow(non_snake_case)]
#![cfg(target_os = "windows")]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bluetooth;
mod config;
mod language;
mod notify;
mod single_instance;
mod startup;
mod theme;
mod tray;

use crate::bluetooth::{
    info::{BluetoothInfo, find_bluetooth_devices, get_bluetooth_devices_info},
    watch::Watcher,
};
use crate::config::{Config, EXE_PATH, TrayIconStyle};
use crate::notify::{NotifyEvent, notify};
use crate::single_instance::SingleInstance;
use crate::theme::{SystemTheme, listen_system_theme};
use crate::tray::{
    convert_tray_info, create_tray,
    icon::{load_app_icon, load_tray_icon},
    menu::{
        MenuGroup, MenuKind, MenuManager,
        handler::MenuHandler,
        item::{SET_ICON_CONNECT_COLOR, SHOW_LOWEST_BATTERY_DEVICE, create_menu},
    },
};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    process::Command,
};

use log::{error, info};
use tray_icon::{TrayIcon, menu::MenuEvent};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _single_instance = SingleInstance::new()?;

    std::panic::set_hook(Box::new(|info| {
        error!("⚠️ Panic: {info}");
        notify(format!("⚠️ Panic: {info}"));
    }));

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        proxy
            .send_event(UserEvent::MenuEvent(event))
            .expect("Failed to send MenuEvent");
    }));

    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy).await;
    event_loop.run_app(&mut app)?;

    Ok(())
}

pub type BluetoothDevicesInfo = Arc<Mutex<HashMap<u64, BluetoothInfo>>>;

struct App {
    bluetooth_devcies_info: BluetoothDevicesInfo,
    config: Arc<Config>,
    watcher: Option<Watcher>,
    event_loop_proxy: EventLoopProxy<UserEvent>,
    exit_threads: Arc<AtomicBool>,
    /// 存储已经通知过的低电量设备（地址），避免再次通知
    notified_devices: Arc<Mutex<HashSet<u64>>>,
    system_theme: Arc<RwLock<SystemTheme>>,
    tray: Mutex<TrayIcon>,
    menu_manager: Mutex<MenuManager>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
}

impl App {
    async fn new(event_loop_proxy: EventLoopProxy<UserEvent>) -> Self {
        let config = Config::open().expect("Failed to open config");

        let (btc_devices, ble_devices) = find_bluetooth_devices()
            .await
            .expect("Failed to find bluetooth devices");

        let bluetooth_devices_info = get_bluetooth_devices_info((&btc_devices, &ble_devices))
            .await
            .expect("Failed to get bluetooth devices info");

        let should_show_lowest_battery_device = config
            .tray_options
            .show_lowest_battery_device
            .load(Ordering::Relaxed);

        // 首次打开软件时，检测有无低电量及需显示最低电量设备
        {
            let mut should_update_tray_icon_style: Option<(u64, u8)> = None;
            for device in bluetooth_devices_info.values() {
                let _ = event_loop_proxy.send_event(UserEvent::Notify(NotifyEvent::LowBattery(
                    device.name.clone(),
                    device.battery,
                    device.address,
                )));

                if device.status && should_show_lowest_battery_device {
                    match should_update_tray_icon_style {
                        Some((ref mut address, ref mut lowest_battery))
                            if device.battery < *lowest_battery =>
                        {
                            *address = device.address;
                            *lowest_battery = device.battery;
                        }
                        None => {
                            should_update_tray_icon_style = Some((device.address, device.battery));
                        }
                        _ => {}
                    }
                }
            }

            if let Some((address, _)) = should_update_tray_icon_style {
                info!("Show Lowest Battery Device on Startup: {}", address);

                if !config
                    .tray_options
                    .tray_icon_style
                    .lock()
                    .unwrap()
                    .update_address(address)
                {
                    // 如果默认是 APP 图标，则切换为数字图标
                    *config.tray_options.tray_icon_style.lock().unwrap() =
                        TrayIconStyle::default_number_icon(address);
                };

                config.save();
            }
        }

        let (tray, menu_manager) =
            create_tray(&config, &bluetooth_devices_info).expect("Failed to create tray");

        Self {
            bluetooth_devcies_info: Arc::new(Mutex::new(bluetooth_devices_info)),
            config: Arc::new(config),
            watcher: None,
            event_loop_proxy,
            exit_threads: Arc::new(AtomicBool::new(false)),
            notified_devices: Arc::new(Mutex::new(HashSet::new())),
            system_theme: Arc::new(RwLock::new(SystemTheme::get())),
            tray: Mutex::new(tray),
            menu_manager: Mutex::new(menu_manager),
            worker_threads: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum UserEvent {
    Exit,
    MenuEvent(MenuEvent),
    Notify(NotifyEvent),
    UnCheckAboutIconMenu,
    UnCheckDeviceMenu,
    UpdateIcon,
    UpdateTray,
    UpdateTrayTooltip,
    Refresh,
    Restart,
}

impl App {
    fn start_watch_devices(&mut self, devices_info: BluetoothDevicesInfo) {
        self.stop_watch_devices();
        let mut watch = Watcher::new(devices_info, self.event_loop_proxy.clone());
        watch.start();
        self.watcher = Some(watch);
    }

    fn stop_watch_devices(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            watcher.stop()
        }
    }

    fn exit(&mut self) {
        self.stop_watch_devices();
        self.exit_threads.store(true, Ordering::Relaxed);
        self.worker_threads
            .drain(..)
            .for_each(|handle| handle.join().expect("Failed to clean thread"));
    }

    fn handle_show_lowest_battery_device(&mut self) {
        let should_show_lowest_battery_device = self
            .config
            .tray_options
            .show_lowest_battery_device
            .load(Ordering::Relaxed);

        if should_show_lowest_battery_device
            && let Some((address, info)) = self
                .bluetooth_devcies_info
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, v)| v.status)
                .min_by_key(|(_, v)| v.battery)
        {
            info!("Show Lowest Battery Device: {}", info.name);

            if !self
                .config
                .tray_options
                .tray_icon_style
                .lock()
                .unwrap()
                .update_address(*address)
            {
                *self.config.tray_options.tray_icon_style.lock().unwrap() =
                    TrayIconStyle::default_number_icon(*address);
            }

            self.config.save();
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        let proxy = self.event_loop_proxy.clone();

        self.start_watch_devices(Arc::clone(&self.bluetooth_devcies_info));

        let exit_threads = Arc::clone(&self.exit_threads);
        let system_theme = Arc::clone(&self.system_theme);
        let theme_handle = listen_system_theme(exit_threads, proxy, system_theme);
        self.worker_threads.push(theme_handle);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            self.exit();
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::UnCheckDeviceMenu => {
                if let Some(menu_map) = self
                    .menu_manager
                    .lock()
                    .unwrap()
                    .get_menus_by_kind(&MenuKind::GroupSingle(MenuGroup::Device, None))
                {
                    menu_map.values().for_each(|m| m.set_checked(false));
                }
            }
            // 取消勾选 [显示最低电量设备] 和 [设置连接配色]
            UserEvent::UnCheckAboutIconMenu => {
                if let Some(menu) = self
                    .menu_manager
                    .lock()
                    .unwrap()
                    .get_menu_by_id(&SHOW_LOWEST_BATTERY_DEVICE)
                {
                    menu.set_checked(false);
                }

                if let Some(menu) = self
                    .menu_manager
                    .lock()
                    .unwrap()
                    .get_menu_by_id(&SET_ICON_CONNECT_COLOR)
                {
                    menu.set_checked(false);
                }
            }
            UserEvent::Exit => {
                self.exit();
                event_loop.exit();
            }
            UserEvent::MenuEvent(event) => {
                let mut menu_manager = self.menu_manager.lock().unwrap();
                menu_manager.handler(event.id(), |is_normal_menu, check_menu| {
                    let menu_handlers = MenuHandler::new(
                        event.id().clone(),
                        is_normal_menu,
                        check_menu,
                        Arc::clone(&self.config),
                        self.event_loop_proxy.clone(),
                    );
                    if let Err(e) = menu_handlers.run() {
                        error!("Failed to handle menu event: {e}")
                    }
                });
            }
            UserEvent::Notify(notify_event) => {
                notify_event.send(&self.config, self.notified_devices.clone())
            }
            UserEvent::UpdateIcon => {
                let current_devices_info = self.bluetooth_devcies_info.lock().unwrap().clone();
                let config = self.config.clone();

                self.handle_show_lowest_battery_device();

                let tray_icon_bt_address = config
                    .tray_options
                    .tray_icon_style
                    .lock()
                    .unwrap()
                    .get_address();

                let icon = tray_icon_bt_address
                    .and_then(|address| current_devices_info.get(&address))
                    .and_then(|info| {
                        load_tray_icon(&config, info.battery, info.status)
                            .inspect_err(|e| error!("Failed to load icon - {e}"))
                            .ok()
                    })
                    .or_else(|| {
                        // 载入图标失败时，需更新配置中的图标样式，注意要在创建菜单之前
                        *config.tray_options.tray_icon_style.lock().unwrap() = TrayIconStyle::App;
                        load_app_icon().ok()
                    });

                let _ = self.tray.lock().unwrap().set_icon(icon);
            }
            UserEvent::UpdateTrayTooltip => {
                let current_devices_info = self.bluetooth_devcies_info.lock().unwrap().clone();
                let bluetooth_tooltip_info = convert_tray_info(&current_devices_info, &self.config);
                let _ = self
                    .tray
                    .lock()
                    .unwrap()
                    .set_tooltip(Some(bluetooth_tooltip_info.join("\n")));
            }
            UserEvent::UpdateTray => {
                let current_devices_info = self.bluetooth_devcies_info.lock().unwrap().clone();
                let config = self.config.clone();

                self.handle_show_lowest_battery_device();

                let tray_menu = match create_menu(&config, &current_devices_info) {
                    Ok((tray_menu, new_menu_manager)) => {
                        let mut menu_manager = self.menu_manager.lock().unwrap();
                        *menu_manager = new_menu_manager;
                        tray_menu
                    }
                    Err(e) => {
                        notify(format!("Failed to create tray menu - {e}"));
                        return;
                    }
                };

                self.tray
                    .lock()
                    .unwrap()
                    .set_menu(Some(Box::new(tray_menu)));
                let _ = self.event_loop_proxy.send_event(UserEvent::UpdateIcon);
                let _ = self
                    .event_loop_proxy
                    .send_event(UserEvent::UpdateTrayTooltip);
            }
            UserEvent::Refresh => {
                let bluetooth_devices_info = futures::executor::block_on(async {
                    let (btc_devices, ble_devices) = find_bluetooth_devices()
                        .await
                        .expect("Failed to find bluetooth devices");

                    get_bluetooth_devices_info((&btc_devices, &ble_devices))
                        .await
                        .expect("Failed to get bluetooth devices info")
                });

                for device in bluetooth_devices_info.values() {
                    let _ = self.event_loop_proxy.send_event(UserEvent::Notify(
                        NotifyEvent::LowBattery(
                            device.name.clone(),
                            device.battery,
                            device.address,
                        ),
                    ));
                }

                {
                    *self.bluetooth_devcies_info.lock().unwrap() = bluetooth_devices_info;
                }

                let _ = self.event_loop_proxy.send_event(UserEvent::UpdateTray);
            }
            UserEvent::Restart => {
                let mut args_os: Vec<OsString> = std::env::args_os().collect();
                args_os.push("--restart".into()); // 添加重启标志（避免与单实例冲突）

                if let Err(e) = Command::new(&*EXE_PATH)
                    .args(args_os.iter().skip(1))
                    .spawn()
                {
                    notify(format!("Failed to restart app: {e}"));
                }

                let _ = self.event_loop_proxy.send_event(UserEvent::Exit);
            }
        }
    }
}
