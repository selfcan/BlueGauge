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
use crate::config::Config;
use crate::notify::{NotifyEvent, notify};
use crate::single_instance::SingleInstance;
use crate::theme::{SystemTheme, listen_system_theme};
use crate::tray::{
    convert_tray_info, create_tray,
    icon::{load_app_icon, load_tray_icon},
    menu_handlers::MenuHandlers,
    menu_item::create_menu,
};

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use log::error;
use tray_icon::menu::MenuId;
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
    let mut app = App::new().await;
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
    tray: Mutex<TrayIcon>,
    tray_check_menus: Mutex<HashMap<MenuId, CheckMenuItem>>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
}

impl App {
    async fn new() -> Self {
        let config = Config::open().expect("Failed to open config");

        let (btc_devices, ble_devices) = find_bluetooth_devices()
            .await
            .expect("Failed to find bluetooth devices");

        let bluetooth_devices_info = get_bluetooth_devices_info((&btc_devices, &ble_devices))
            .await
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
            tray: Mutex::new(tray),
            tray_check_menus: Mutex::new(tray_check_menus),
            worker_threads: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum UserEvent {
    Exit,
    MenuEvent(MenuEvent),
    Notify(NotifyEvent),
    UpdateTray,
}

impl App {
    fn add_proxy(&mut self, event_loop_proxy: Option<EventLoopProxy<UserEvent>>) -> &mut Self {
        self.event_loop_proxy = event_loop_proxy;
        self
    }

    fn start_watch_devices(&mut self, devices_info: BluetoothDevicesInfo) {
        self.stop_watch_devices();
        let mut watch = Watcher::new(devices_info, self.event_loop_proxy.clone().unwrap());
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
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        let proxy = self.event_loop_proxy.clone().expect("Failed to get proxy");

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
            UserEvent::Exit => {
                self.exit();
                event_loop.exit();
            }
            UserEvent::MenuEvent(event) => {
                let config = Arc::clone(&self.config);
                let tray_check_menus = self.tray_check_menus.lock().unwrap().clone();

                let menu_id = event.id();
                let menu_handlers = MenuHandlers::new(
                    menu_id.clone(),
                    Arc::clone(&config),
                    self.event_loop_proxy.clone().unwrap(),
                    tray_check_menus,
                );
                menu_handlers.run();
            }
            UserEvent::Notify(notify_event) => {
                notify_event.send(&self.config, self.notified_devices.clone())
            }
            UserEvent::UpdateTray => {
                let current_devices_info = self.bluetooth_devcies_info.lock().unwrap().clone();
                let config = self.config.clone();

                let tray = self.tray.lock().unwrap();

                let tray_menu = match create_menu(&config, &current_devices_info) {
                    Ok((tray_menu, new_tray_check_menus)) => {
                        let mut tray_check_menus = self.tray_check_menus.lock().unwrap();
                        *tray_check_menus = new_tray_check_menus;
                        tray_menu
                    }
                    Err(e) => {
                        notify(format!("Failed to create tray menu - {e}"));
                        return;
                    }
                };
                tray.set_menu(Some(Box::new(tray_menu)));

                let bluetooth_tooltip_info = convert_tray_info(&current_devices_info, &config);
                tray.set_tooltip(Some(bluetooth_tooltip_info.join("\n")))
                    .expect("Failed to set tray tooltip");

                let tray_icon_bt_address = config
                    .tray_options
                    .tray_icon_style
                    .lock()
                    .unwrap()
                    .get_address();
                let icon = tray_icon_bt_address
                    .and_then(|address| current_devices_info.get(&address))
                    .and_then(|info| load_tray_icon(&config, info.battery, info.status).ok())
                    .or_else(|| load_app_icon().ok());
                let _ = tray.set_icon(icon);
            }
        }
    }
}
