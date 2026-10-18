//! Keyboard event handler for vim mode processing

pub mod double_tap;
mod shortcuts;

use std::sync::{Arc, Mutex};

use crate::commands::RecordedKey;
use crate::config::{DoubleTapModifier, Settings};
use crate::keyboard::{KeyCode, KeyEvent};
use crate::nvim_edit::EditSessionManager;
use crate::vim::VimState;

use double_tap::{DoubleTapKey, DoubleTapManager};
use shortcuts::{check_nvim_edit_shortcut, check_vim_key, process_vim_input};

/// Callback type for when a double-tap triggers a mode activation
pub type DoubleTapCallback = Box<dyn Fn(DoubleTapKey) + Send + 'static>;

/// Shared state and callbacks used while handling keyboard events.
pub struct KeyboardCallbackContext {
    pub vim_state: Arc<Mutex<VimState>>,
    pub settings: Arc<Mutex<Settings>>,
    pub record_key_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<RecordedKey>>>>,
    pub edit_session_manager: Arc<EditSessionManager>,
    pub double_tap_manager: Arc<Mutex<DoubleTapManager>>,
    pub double_tap_callback: DoubleTapCallback,
}

/// Create the keyboard callback that processes key events
pub fn create_keyboard_callback(
    context: KeyboardCallbackContext,
) -> impl Fn(KeyEvent) -> Option<KeyEvent> + Send + 'static {
    let KeyboardCallbackContext {
        vim_state,
        settings,
        record_key_tx,
        edit_session_manager,
        double_tap_manager,
        double_tap_callback,
    } = context;

    move |event| {
        // Reset modifier double-tap trackers when any non-modifier key is pressed.
        // This prevents false double-tap detection when using shortcuts like CMD+C
        // followed quickly by CMD+V (which would otherwise look like two CMD taps).
        if event.is_key_down {
            if let Some(keycode) = event.keycode() {
                match keycode {
                    KeyCode::Escape => {}
                    _ => {
                        let mut dt_manager = double_tap_manager.lock().unwrap();
                        dt_manager.command_tracker.reset();
                        dt_manager.option_tracker.reset();
                        dt_manager.control_tracker.reset();
                        dt_manager.shift_tracker.reset();
                    }
                }
            }
        }

        // Check for Escape key double-tap (for non-modifier double-tap shortcuts)
        if let Some(keycode) = event.keycode() {
            if keycode == KeyCode::Escape {
                let mut dt_manager = double_tap_manager.lock().unwrap();
                if let Some(double_tap_key) = dt_manager.process_key_event(DoubleTapKey::Escape, event.is_key_down) {
                    let nvim_uses_escape = settings.lock().unwrap().nvim_edit.double_tap_modifier
                        == DoubleTapModifier::Escape;

                    if nvim_uses_escape {
                        double_tap_callback(double_tap_key);
                        return None; // Suppress the escape key
                    }
                }
            }
        }

        // Check if we're recording a key (only on key down)
        if event.is_key_down {
            if let Some(recorded) = try_record_key(&event, &record_key_tx) {
                let mut record_tx = record_key_tx.lock().unwrap();
                if let Some(tx) = record_tx.take() {
                    let _ = tx.send(recorded);
                    return None;
                }
            }
        }

        // Check shortcuts on key down
        if event.is_key_down {
            let settings_guard = settings.lock().unwrap();

            // Check nvim edit shortcut
            if let Some(result) = check_nvim_edit_shortcut(
                &event,
                &settings_guard,
                Arc::clone(&edit_session_manager),
                Arc::clone(&settings),
            ) {
                return result;
            }

            // Check vim key
            if let Some(result) = check_vim_key(&event, &settings_guard, Arc::clone(&vim_state)) {
                return result;
            }
        }

        // Process normal vim input
        process_vim_input(event, &settings, &vim_state)
    }
}

/// Try to record a key if recording is active
fn try_record_key(
    event: &KeyEvent,
    record_key_tx: &Arc<Mutex<Option<tokio::sync::oneshot::Sender<RecordedKey>>>>,
) -> Option<RecordedKey> {
    use crate::commands::RecordedModifiers;

    let record_tx = record_key_tx.lock().unwrap();
    if record_tx.is_some() {
        if let Some(keycode) = event.keycode() {
            return Some(RecordedKey {
                name: keycode.to_name().to_string(),
                display_name: keycode.to_display_name().to_string(),
                modifiers: RecordedModifiers {
                    shift: event.modifiers.shift,
                    control: event.modifiers.control,
                    option: event.modifiers.option,
                    command: event.modifiers.command,
                },
            });
        }
    }
    None
}
