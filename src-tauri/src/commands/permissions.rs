//! Permission-related Tauri commands

use tauri::{AppHandle, Manager};

use crate::keyboard::check_accessibility_permission;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
}

#[tauri::command]
pub fn get_permission_status() -> PermissionStatus {
    PermissionStatus {
        accessibility: check_accessibility_permission(),
    }
}

#[tauri::command]
pub fn complete_permission_setup(app: AppHandle) -> Result<(), String> {
    if !check_accessibility_permission() {
        return Err("Accessibility permission must be allowed before setup can finish".to_string());
    }

    app.run_on_main_thread(crate::window::hide_permission_drag_helper)
        .map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window("permissions") {
        window.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_accessibility_settings(app: AppHandle) -> Result<(), String> {
    use std::process::Command;

    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|error| error.to_string())?;

    app.run_on_main_thread(|| {
        if let Err(error) = crate::window::show_permission_drag_helper() {
            log::warn!("Could not show permission drag helper: {error}");
        }
    })
    .map_err(|error| error.to_string())
}
