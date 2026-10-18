//! Settings-related Tauri commands

use tauri::{AppHandle, State};

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
    // Saved filetypes live in their own file and are never deserialized from the window.
    let domain_filetypes = std::mem::take(&mut settings.nvim_edit.domain_filetypes);
    *settings = new_settings.clone();
    settings.nvim_edit.domain_filetypes = domain_filetypes;
    settings.save()?;
    drop(settings);

    crate::apply_activation_policy(&app, new_settings.show_in_dock)?;
    crate::shortcut::register(&app, &new_settings.nvim_edit);
    Ok(())
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
