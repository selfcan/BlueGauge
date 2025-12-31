use crate::{config::EXE_NAME, util::to_wide};

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
        let exe_name = EXE_NAME.as_str();

        let mut mutex_name = std::ffi::OsString::from("Global\\");
        mutex_name.push(exe_name);
        mutex_name.push("AppMutex");

        let name = to_wide(mutex_name);

        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .context("Failed to create single instance mutex.")?;

        let single_instance = Self { handle };

        if single_instance.handle.is_invalid() {
            return Err(anyhow!(
                "Failed to create single instance mutex: {:?}",
                unsafe { GetLastError() }
            ));
        }

        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // 如果是重启操作，跳过单实例检查
            let args: Vec<String> = std::env::args().collect();
            let is_restart = args.iter().any(|arg| arg == "--restart");
            if is_restart {
                return Ok(single_instance);
            }
            return Err(anyhow!("BlueGauge already running, exit the new process"));
        }

        Ok(single_instance)
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
