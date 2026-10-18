//! Edit Popup shortcut commands

use std::str::FromStr;

use tauri::{AppHandle, State};
use tauri_plugin_global_shortcut::Code;

use crate::keyboard::KeyCode;
use crate::AppState;

/// A key that can be used in the Edit Popup shortcut
#[derive(Debug, Clone, serde::Serialize)]
pub struct ShortcutKey {
    pub name: String,
    pub display_name: String,
}

#[tauri::command]
pub fn get_key_display_name(key_name: String) -> Option<String> {
    KeyCode::from_name(&key_name).map(|k| k.to_display_name().to_string())
}

/// Map a DOM `KeyboardEvent.code` (e.g. "KeyE") to the key stored in settings
#[tauri::command]
pub fn shortcut_key_from_code(code: String) -> Option<ShortcutKey> {
    let key = KeyCode::from_code(Code::from_str(&code).ok()?)?;
    Some(ShortcutKey {
        name: key.to_name().to_string(),
        display_name: key.to_display_name().to_string(),
    })
}

/// Release the shortcut while the settings window records a new one, so the
/// key press reaches the window instead of opening the popup.
#[tauri::command]
pub fn set_shortcut_paused(app: AppHandle, state: State<AppState>, paused: bool) {
    if paused {
        crate::shortcut::unregister(&app);
    } else {
        let nvim_edit = state.settings.lock().unwrap().nvim_edit.clone();
        crate::shortcut::register(&app, &nvim_edit);
    }
}

#[tauri::command]
pub fn get_shortcut_error() -> Option<String> {
    crate::shortcut::registration_error()
}
