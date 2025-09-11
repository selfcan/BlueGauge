use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;

use anyhow::{Context, Result, anyhow};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::{CreateMutexW, ReleaseMutex},
    },
    core::PCWSTR,
};

pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    /// Creates a new system-wide mutex to ensure that only one instance of
    /// the application is running.
    pub fn new() -> Result<Self> {
        let exe_name = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|n| n.to_owned()))
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "BlueGauge".to_owned());

        let mut mutex_name = OsString::from("Global\\");
        mutex_name.push(exe_name);
        mutex_name.push("AppMutex");

        let name: Vec<u16> = mutex_name
            .encode_wide()
            .chain(std::iter::once(0)) // 结尾 0，C 风格字符串
            .collect();

        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .context("Failed to create single instance mutex.")?;

        if handle.is_invalid() {
            return Err(anyhow!(
                "Failed to create single instance mutex: {:?}",
                unsafe { GetLastError() }
            ));
        }

        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(anyhow!("BlueGauge already running, exit the new process"));
        }

        Ok(Self { handle })
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}
