//! Global shortcut that opens the Edit Popup.
//!
//! The shortcut is registered as a system hotkey, so ovim never sees other
//! keystrokes and needs no Input Monitoring permission.

use std::sync::{Arc, Mutex};

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::config::NvimEditSettings;
use crate::keyboard::KeyCode;
use crate::{nvim_edit, AppState};

static REGISTRATION_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                open_edit_popup(app);
            }
        })
        .build()
}

fn open_edit_popup(app: &AppHandle) {
    let state = app.state::<AppState>();
    let nvim_settings = state.settings.lock().unwrap().nvim_edit.clone();
    if !nvim_settings.enabled {
        return;
    }

    let manager = Arc::clone(&state.edit_session_manager);
    let settings = Arc::clone(&state.settings);
    std::thread::spawn(move || {
        if let Err(e) = nvim_edit::trigger_nvim_edit(manager, nvim_settings, settings) {
            log::error!("Failed to open edit popup: {}", e);
        }
    });
}

/// Register the configured shortcut, replacing any previous one.
pub fn register(app: &AppHandle, settings: &NvimEditSettings) {
    unregister(app);

    let result = if settings.enabled {
        to_shortcut(settings).and_then(|shortcut| {
            app.global_shortcut()
                .register(shortcut)
                .map_err(|e| format!("Could not register the shortcut: {}", e))
        })
    } else {
        Ok(())
    };

    match &result {
        Ok(()) if settings.enabled => log::info!(
            "Registered Edit Popup shortcut {:?} + {}",
            settings.shortcut_modifiers,
            settings.shortcut_key
        ),
        Ok(()) => log::info!("Edit Popup is disabled, no shortcut registered"),
        Err(e) => log::error!("{}", e),
    }
    let error = result.err();
    let _ = app.emit("shortcut-error", &error);
    *REGISTRATION_ERROR.lock().unwrap() = error;
}

pub fn unregister(app: &AppHandle) {
    if let Err(e) = app.global_shortcut().unregister_all() {
        log::warn!("Failed to unregister shortcuts: {}", e);
    }
}

/// Error from the last registration attempt, shown in the settings window
pub fn registration_error() -> Option<String> {
    REGISTRATION_ERROR.lock().unwrap().clone()
}

fn to_shortcut(settings: &NvimEditSettings) -> Result<Shortcut, String> {
    let key = KeyCode::from_name(&settings.shortcut_key)
        .ok_or_else(|| format!("Unsupported shortcut key '{}'", settings.shortcut_key))?;

    let held = &settings.shortcut_modifiers;
    let mut modifiers = Modifiers::empty();
    modifiers.set(Modifiers::SHIFT, held.shift);
    modifiers.set(Modifiers::CONTROL, held.control);
    modifiers.set(Modifiers::ALT, held.option);
    modifiers.set(Modifiers::SUPER, held.command);

    Ok(Shortcut::new(Some(modifiers), key.to_code()))
}
