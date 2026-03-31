#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

const APP_NAME: &str = "TaskPilot";

#[cfg(target_os = "windows")]
pub fn enable_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let key = hkcu.open_subkey_with_flags(path, KEY_WRITE)?;

    // Get current executable path
    let exe_path = std::env::current_exe()?;
    let command = format!("\"{}\" --minimized", exe_path.display());

    key.set_value(APP_NAME, &command)?;
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn disable_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let key = hkcu.open_subkey_with_flags(path, KEY_WRITE)?;

    key.delete_value(APP_NAME).ok(); // Ignore error if key doesn't exist
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn is_autostart_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";

    if let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_READ) {
        if let Ok(value) = key.get_value::<String, _>(APP_NAME) {
            return !value.is_empty();
        }
    }
    false
}

// Non-Windows platforms: no-op implementations
#[cfg(not(target_os = "windows"))]
pub fn enable_autostart() -> Result<(), Box<dyn std::error::Error>> {
    Err("Auto-start is only supported on Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn disable_autostart() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn is_autostart_enabled() -> bool {
    false
}
