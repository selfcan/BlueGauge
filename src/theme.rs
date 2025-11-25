use crate::UserEvent;

use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

use image::Rgba;
use winit::event_loop::EventLoopProxy;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
};

const PERSONALIZE_REGISTRY_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const SYSTEM_USES_LIGHT_THEME_REGISTRY_KEY: &str = "SystemUsesLightTheme";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemTheme {
    Light,
    Dark,
}

impl SystemTheme {
    pub fn get() -> Self {
        let personalize_reg_key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(PERSONALIZE_REGISTRY_KEY, KEY_READ | KEY_WRITE)
            .expect("This program requires Windows 10 14393 or above");

        let theme_reg_value: u32 = personalize_reg_key
            .get_value(SYSTEM_USES_LIGHT_THEME_REGISTRY_KEY)
            .expect("This program requires Windows 10 14393 or above");

        match theme_reg_value {
            0 => SystemTheme::Dark,
            _ => SystemTheme::Light,
        }
    }

    pub fn get_font_color(&self) -> Rgba<u8> {
        match self {
            Self::Dark => Rgba([255, 255, 255, 255]),
            Self::Light => Rgba([31, 31, 31, 255]),
        }
    }
}

pub fn listen_system_theme(
    exit_threads: Arc<AtomicBool>,
    proxy: EventLoopProxy<UserEvent>,
    system_theme: Arc<RwLock<SystemTheme>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        loop {
            let original_system_theme = {
                let system_theme = system_theme.read().unwrap();
                *system_theme
            };

            let current_system_theme = SystemTheme::get();

            if original_system_theme != current_system_theme {
                let mut system_theme = system_theme.write().unwrap();
                *system_theme = current_system_theme;

                proxy
                    .send_event(UserEvent::UpdateTray)
                    .expect("Failed to send UpdateTray Event");
            }

            for _ in 0..=5 {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if exit_threads.load(Ordering::Relaxed) {
                    return;
                }
            }
        }
    })
}
