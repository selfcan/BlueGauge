use anyhow::{Context, Result, anyhow};
use winreg::RegKey;
use winreg::enums::*;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn get_exe_name_and_path() -> Result<(String, String)> {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_stem()
                .map(|n| (n.to_owned(), p.to_string_lossy().into_owned()))
        })
        .map(|(n, p)| (n.to_string_lossy().into_owned(), p))
        .ok_or_else(|| anyhow!("Failed to convert exe path to string"))
}

pub fn set_startup(enabled: bool) -> Result<()> {
    let (exe_name, exe_path) = get_exe_name_and_path()?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _disp) = hkcu.create_subkey(RUN_KEY)?;

    if enabled {
        run_key
            .set_value(exe_name, &exe_path)
            .with_context(|| "Failed to set the autostart registry key")?;
    } else {
        run_key
            .delete_value(exe_name)
            .with_context(|| "Failed to delete the autostart registry key")?;
    }

    Ok(())
}

pub fn get_startup_status() -> Result<bool> {
    let (exe_name, exe_path) = get_exe_name_and_path()?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .map_err(|e| anyhow!("Failed to open HKEY_CURRENT_USER\\...\\Run - {e}"))?;

    match run_key.get_value::<String, _>(exe_name) {
        Ok(value) => Ok(value == exe_path),
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(anyhow!("Failed to get the autostart registry key - {e}")),
    }
}
