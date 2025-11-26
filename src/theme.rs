use crate::{UserEvent, util::to_wide};

use std::ptr::null_mut;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

use image::Rgba;
use log::{error, info};
use windows::{
    Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Security::SECURITY_ATTRIBUTES,
        System::{
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_DWORD, REG_NOTIFY_CHANGE_LAST_SET,
                RRF_RT_REG_DWORD, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue,
                RegOpenKeyExW,
            },
            Threading::{CreateEventW, WaitForSingleObject},
        },
    },
    core::PCWSTR,
};
use winit::event_loop::EventLoopProxy;

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
        let path = to_wide(PERSONALIZE_REGISTRY_KEY);
        let name = to_wide(SYSTEM_USES_LIGHT_THEME_REGISTRY_KEY);

        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let mut reg_dword = REG_DWORD;

        let ret = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_DWORD,
                Some(&mut reg_dword),
                Some(&mut value as *mut _ as *mut _),
                Some(&mut size as *mut _),
            )
        };

        if ret.is_err() {
            SystemTheme::Light
        } else {
            match value {
                0 => SystemTheme::Dark,
                _ => SystemTheme::Light,
            }
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
        unsafe {
            let mut hkey: HKEY = HKEY(std::ptr::null_mut());
            let path = to_wide(PERSONALIZE_REGISTRY_KEY);

            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                KEY_NOTIFY,
                &mut hkey,
            );

            if status.0 != 0 {
                eprintln!("Failed to open registry key: {}", status.0);
                return;
            }

            let registry_event =
                CreateEventW(Some(null_mut::<SECURITY_ATTRIBUTES>()), true, false, None);

            let Ok(handle) = registry_event else {
                error!("Failed to create event");
                return;
            };

            loop {
                let status = RegNotifyChangeKeyValue(
                    hkey,
                    false,
                    REG_NOTIFY_CHANGE_LAST_SET,
                    Some(handle),
                    true, // 异步模式
                );

                if status.is_err() {
                    error!("RegNotifyChangeKeyValue failed: {}", status.0);
                    break;
                }

                let timeout = 1500; // milliseconds
                let wait_event = WaitForSingleObject(handle, timeout);

                match wait_event {
                    // registry changed
                    WAIT_OBJECT_0 => {
                        let original_system_theme = {
                            let system_theme = system_theme.read().unwrap();
                            *system_theme
                        };

                        let current_system_theme = SystemTheme::get();

                        if original_system_theme != current_system_theme {
                            info!("System Theme changed = {current_system_theme:?}");

                            let mut system_theme = system_theme.write().unwrap();
                            *system_theme = current_system_theme;

                            proxy
                                .send_event(UserEvent::UpdateTray)
                                .expect("Failed to send UpdateTray Event");
                        }
                    }

                    // quit_event triggered
                    WAIT_TIMEOUT => {
                        if exit_threads.load(Ordering::Relaxed) {
                            info!("Exit flag detected, stopping watcher...");
                            break;
                        }
                    }

                    other => {
                        error!("WaitForSingleObject error: {}", other.0);
                        break;
                    }
                }
            }

            let _ = CloseHandle(handle);
            let _ = RegCloseKey(hkey);
        }
    })
}
