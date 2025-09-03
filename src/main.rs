#![allow(non_snake_case)]
#![cfg(target_os = "windows")]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bluetooth;
mod config;
mod icon;
mod language;
mod menu_handlers;
mod notify;
mod startup;
mod theme;
mod tray;

use crate::bluetooth::info::{BluetoothInfo, find_bluetooth_devices, get_bluetooth_devices_info};
use crate::bluetooth::watch::Watcher;
use crate::config::*;
use crate::icon::{load_app_icon, load_battery_icon};
use crate::menu_handlers::MenuHandlers;
use crate::notify::{NotifyEvent, notify};
use crate::theme::{SystemTheme, listen_system_theme};
use crate::tray::{convert_tray_info, create_menu, create_tray};

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use log::error;
use tray_icon::{
    TrayIcon,
    menu::{CheckMenuItem, MenuEvent},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

fn main() -> anyhow::Result<()> {
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

    let mut app = App::default();
    let proxy = event_loop.create_proxy();
    app.add_proxy(Some(proxy));

    event_loop.run_app(&mut app)?;

    Ok(())
}

pub type BluetoothDevicesInfo = Arc<Mutex<HashMap<u64, BluetoothInfo>>>;

struct App {
    bluetooth_devcies_info: BluetoothDevicesInfo,
    config: Arc<Config>,
    watcher: Option<Watcher>,
    event_loop_proxy: Option<EventLoopProxy<UserEvent>>,
    exit_threads: Arc<AtomicBool>,
    /// 存储已经通知过的低电量设备（地址），避免再次通知
    notified_devices: Arc<Mutex<HashSet<u64>>>,
    system_theme: Arc<RwLock<SystemTheme>>,
    tray: Mutex<Option<TrayIcon>>,
    tray_check_menus: Mutex<Option<Vec<CheckMenuItem>>>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
}

impl Default for App {
    fn default() -> Self {
        let config = Config::open().expect("Failed to open config");

        let (btc_devices, ble_devices) =
            find_bluetooth_devices().expect("Failed to find bluetooth devices");
        let bluetooth_devices_info = get_bluetooth_devices_info((&btc_devices, &ble_devices))
            .expect("Failed to get bluetooth devices info");

        let (tray, tray_check_menus) =
            create_tray(&config, &bluetooth_devices_info).expect("Failed to create tray");

        Self {
            bluetooth_devcies_info: Arc::new(Mutex::new(bluetooth_devices_info)),
            config: Arc::new(config),
            watcher: None,
            event_loop_proxy: None,
            exit_threads: Arc::new(AtomicBool::new(false)),
            notified_devices: Arc::new(Mutex::new(HashSet::new())),
            system_theme: Arc::new(RwLock::new(SystemTheme::get())),
            tray: Mutex::new(Some(tray)),
            tray_check_menus: Mutex::new(Some(tray_check_menus)),
            worker_threads: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum UserEvent {
    Exit,
    MenuEvent(MenuEvent),
    Notify(NotifyEvent),
    UnpdatTray,
}

impl App {
    fn add_proxy(&mut self, event_loop_proxy: Option<EventLoopProxy<UserEvent>>) -> &mut Self {
        self.event_loop_proxy = event_loop_proxy;
        self
    }

    fn start_watch_device(&mut self, devices_info: BluetoothDevicesInfo) {
        // 如果已有一个监控任务在运行，先停止它
        self.stop_watch_device();

        if let Some(proxy) = &self.event_loop_proxy {
            match Watcher::start(devices_info, proxy.clone()) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(e) => error!("Failed to start the bluetooth watch: {e}"),
            }
        }
    }

    fn stop_watch_device(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            watcher.stop()
        }
    }

    fn exit(&mut self) {
        self.stop_watch_device();

        self.exit_threads.store(true, Ordering::Relaxed);
        self.worker_threads
            .drain(..)
            .for_each(|handle| handle.join().expect("Failed to clean thread"));
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        let proxy = self.event_loop_proxy.clone().expect("Failed to get proxy");

        self.start_watch_device(Arc::clone(&self.bluetooth_devcies_info));

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
            UserEvent::Exit => {
                self.exit();
                event_loop.exit();
            }
            UserEvent::MenuEvent(event) => {
                let config = Arc::clone(&self.config);
                let tray_check_menus = self
                    .tray_check_menus
                    .lock()
                    .unwrap()
                    .clone()
                    .expect("Tray check menus not initialized");

                let menu_event_id = event.id().as_ref();
                match menu_event_id {
                    "quit" => MenuHandlers::exit(self.event_loop_proxy.clone().unwrap()),
                    "restart" => MenuHandlers::restart(self.event_loop_proxy.clone().unwrap()),
                    "startup" => MenuHandlers::startup(tray_check_menus),
                    "open_config" => MenuHandlers::open_config(),
                    "set_icon_connect_color" => MenuHandlers::set_icon_connect_color(
                        &config,
                        menu_event_id,
                        self.event_loop_proxy.clone().unwrap(),
                        tray_check_menus,
                    ),
                    // 通知设置：低电量
                    "0.01" | "0.05" | "0.10" | "0.15" | "0.20" | "0.25" | "0.30" => {
                        MenuHandlers::set_notify_low_battery(
                            &config,
                            menu_event_id,
                            tray_check_menus,
                        );
                    }
                    // 通知设置：静音/断开连接/重新连接/添加/删除
                    "disconnection" | "reconnection" | "added" | "removed" => {
                        MenuHandlers::set_notify_device_change(
                            &config,
                            menu_event_id,
                            tray_check_menus,
                        );
                    }
                    // 托盘设置：提示内容设置
                    "show_disconnected" | "truncate_name" | "prefix_battery" => {
                        MenuHandlers::set_tray_tooltip(
                            &config,
                            menu_event_id,
                            self.event_loop_proxy.clone().unwrap(),
                            tray_check_menus,
                        );
                    }
                    // 托盘设置：选择图标
                    _ => {
                        MenuHandlers::set_tray_icon_source(
                            &config,
                            menu_event_id,
                            self.event_loop_proxy.clone().unwrap(),
                            tray_check_menus,
                        );
                    }
                }
            }
            UserEvent::Notify(notify_event) => {
                notify_event.send(&self.config, self.notified_devices.clone())
            }
            UserEvent::UnpdatTray => {
                let cuurent_devices_info = self.bluetooth_devcies_info.lock().unwrap().clone();
                let config = self.config.clone();

                let (tray_menu, new_tray_check_menus) =
                    match create_menu(&config, &cuurent_devices_info) {
                        Ok(menu) => menu,
                        Err(e) => {
                            notify(format!("Failed to create tray menu - {e}"));
                            return;
                        }
                    };

                if let Some(tray_check_menus) = self.tray_check_menus.lock().unwrap().as_mut() {
                    *tray_check_menus = new_tray_check_menus;
                }

                let bluetooth_tooltip_info = convert_tray_info(&cuurent_devices_info, &config);

                if let Some(tray) = self.tray.lock().unwrap().as_mut() {
                    tray.set_menu(Some(Box::new(tray_menu)));

                    let _ = tray.set_tooltip(Some(bluetooth_tooltip_info.join("\n")));

                    let tray_icon_bt_address = config
                        .tray_options
                        .tray_icon_source
                        .lock()
                        .unwrap()
                        .get_address();

                    let icon = tray_icon_bt_address
                        .and_then(|address| cuurent_devices_info.get(&address))
                        .and_then(|info| load_battery_icon(&config, info.battery, info.status).ok())
                        .or_else(|| load_app_icon().ok());

                    let _ = tray.set_icon(icon);
                }
            }
        }
    }
}
