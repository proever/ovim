//! Settings-related Tauri commands

use tauri::{AppHandle, Emitter, Manager, State};

use crate::config::Settings;
use crate::AppState;

#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    let settings = state.settings.lock().unwrap();
    settings.clone()
}

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<AppState>,
    new_settings: Settings,
) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    *settings = new_settings.clone();
    settings.save()?;

    crate::apply_activation_policy(&app, new_settings.show_in_dock)?;
    let _ = app.emit("settings-changed", new_settings);
    Ok(())
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pick_app(app: AppHandle) -> Result<Option<String>, String> {
    use std::process::Command;

    let script = r#"
        set appPath to choose file of type {"app"} with prompt "Select an application" default location "/Applications"
        set appPath to POSIX path of appPath
        return appPath
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    // Refocus settings window after picker closes
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
    }

    if !output.status.success() {
        return Ok(None);
    }

    let app_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if app_path.is_empty() {
        return Ok(None);
    }

    let bundle_output = Command::new("mdls")
        .args(["-name", "kMDItemCFBundleIdentifier", "-raw", &app_path])
        .output()
        .map_err(|e| format!("Failed to get bundle ID: {}", e))?;

    let bundle_id = String::from_utf8_lossy(&bundle_output.stdout)
        .trim()
        .to_string();
    if bundle_id.is_empty() || bundle_id == "(null)" {
        return Err("Could not determine bundle identifier".to_string());
    }

    Ok(Some(bundle_id))
}

#[tauri::command]
pub fn remove_domain_filetype(
    state: State<AppState>,
    domain: String,
) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.nvim_edit.remove_filetype_for_domain(&domain);
    // Note: settings.save() doesn't save domain_filetypes (they're in separate file)
    // remove_filetype_for_domain already saves to domain-filetypes.yaml
    Ok(())
}

/// Get all domain filetypes (stored in separate file from main settings)
#[tauri::command]
pub fn get_domain_filetypes(state: State<AppState>) -> std::collections::HashMap<String, String> {
    let settings = state.settings.lock().unwrap();
    settings.nvim_edit.get_all_domain_filetypes().clone()
}
