// Allow unexpected_cfgs from the objc crate's macros which use cfg(feature = "cargo-clippy")
#![allow(unexpected_cfgs)]

mod commands;
mod config;
mod keyboard;
mod keyboard_handler;
mod nvim_edit;
mod vim;
mod widgets;
mod window;

use std::sync::{Arc, Mutex};

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIcon,
    AppHandle, Emitter, Listener, Manager, State,
};

use commands::RecordedKey;
use config::{DoubleTapModifier, Settings};
use keyboard::{
    check_accessibility_permission, check_input_monitoring_permission, KeyboardCapture,
};
use keyboard_handler::{create_keyboard_callback, KeyboardCallbackContext};
use keyboard_handler::double_tap::{DoubleTapKey, DoubleTapManager};
use nvim_edit::prewarm::PrewarmManager;
use nvim_edit::EditSessionManager;
use vim::VimState;
use window::setup_indicator_window;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

static LOG_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Get a reference to the global app handle (for emitting events from keyboard handler)
pub fn get_app_handle() -> Option<&'static AppHandle> {
    APP_HANDLE.get()
}

fn init_file_logger() {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("/tmp/ovim-rust.log")
        .expect("Failed to create log file");

    LOG_FILE.set(Mutex::new(file)).ok();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let timestamp = chrono::Local::now().format("%H:%M:%S%.3f");
            let line = format!(
                "[{}] {} - {}\n",
                timestamp,
                record.level(),
                record.args()
            );

            if let Some(file_mutex) = LOG_FILE.get() {
                if let Ok(mut file) = file_mutex.lock() {
                    let _ = file.write_all(line.as_bytes());
                    let _ = file.flush();
                }
            }

            write!(buf, "{}", line)
        })
        .init();
}

/// Application state shared across commands
pub struct AppState {
    pub settings: Arc<Mutex<Settings>>,
    pub vim_state: Arc<Mutex<VimState>>,
    pub keyboard_capture: KeyboardCapture,
    pub record_key_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<RecordedKey>>>>,
    #[allow(dead_code)]
    edit_session_manager: Arc<EditSessionManager>,
}

/// Helper to check if a double-tap key matches a setting
fn matches_double_tap_setting(setting: &DoubleTapModifier, key: &DoubleTapKey) -> bool {
    matches!(
        (setting, key),
        (DoubleTapModifier::Command, DoubleTapKey::Command)
            | (DoubleTapModifier::Option, DoubleTapKey::Option)
            | (DoubleTapModifier::Control, DoubleTapKey::Control)
            | (DoubleTapModifier::Shift, DoubleTapKey::Shift)
            | (DoubleTapModifier::Escape, DoubleTapKey::Escape)
    )
}

/// Handle double-tap activation for nvim edit
fn handle_double_tap_activation(
    double_tap_key: DoubleTapKey,
    settings: &Arc<Mutex<Settings>>,
    edit_session_manager: &Arc<EditSessionManager>,
) {
    let settings_guard = settings.lock().unwrap();

    let nvim_edit_trigger = matches_double_tap_setting(
        &settings_guard.nvim_edit.double_tap_modifier,
        &double_tap_key,
    );

    if nvim_edit_trigger && settings_guard.nvim_edit.enabled {
        log::info!("Double-tap {:?} detected - activating nvim edit", double_tap_key);
        let nvim_settings = settings_guard.nvim_edit.clone();
        let shared_settings = Arc::clone(settings);
        drop(settings_guard);

        // Trigger nvim edit
        let manager = Arc::clone(edit_session_manager);
        std::thread::spawn(move || {
            if let Err(e) = nvim_edit::trigger_nvim_edit(manager, nvim_settings, Some(shared_settings)) {
                log::error!("Failed to trigger nvim edit via double-tap: {}", e);
            }
        });
    }
}

fn update_tray_icon(tray: &TrayIcon, mode: &str, show_mode: bool) {
    let icon_bytes: &[u8] = if show_mode {
        match mode {
            "insert" => include_bytes!("../icons/tray-icon-insert.png"),
            "normal" => include_bytes!("../icons/tray-icon-normal.png"),
            "visual" => include_bytes!("../icons/tray-icon-visual.png"),
            _ => include_bytes!("../icons/tray-icon.png"),
        }
    } else {
        include_bytes!("../icons/tray-icon.png")
    };

    match image::load_from_memory(icon_bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            let icon = Image::new_owned(rgba.into_raw(), width, height);
            if let Err(e) = tray.set_icon(Some(icon)) {
                log::error!("Failed to set tray icon: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to decode tray icon: {}", e);
        }
    }
}

fn show_settings_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("settings") else {
        log::error!("Settings window is unavailable");
        return;
    };

    if let Err(error) = window.show() {
        log::error!("Failed to show settings window: {}", error);
        return;
    }

    if let Err(error) = window.set_focus() {
        log::error!("Failed to focus settings window: {}", error);
    }
}

#[cfg(target_os = "macos")]
fn apply_activation_policy(app: &AppHandle, show_in_dock: bool) -> Result<(), String> {
    let policy = if show_in_dock {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    app.set_activation_policy(policy)
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn apply_activation_policy(_app: &AppHandle, _show_in_dock: bool) -> Result<(), String> {
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_file_logger();
    log::info!("ovim-rust started");

    let (vim_state, mode_rx) = VimState::new();
    let vim_state = Arc::new(Mutex::new(vim_state));

    let settings = Arc::new(Mutex::new(Settings::load()));

    let record_key_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<RecordedKey>>>> =
        Arc::new(Mutex::new(None));
    let mut edit_session_manager = EditSessionManager::new();

    // Initialize prewarm manager if enabled and using alacritty
    {
        let s = settings.lock().unwrap();
        if s.nvim_edit.prewarm_terminal && s.nvim_edit.terminal == "alacritty" {
            log::info!("Pre-warming terminal enabled, spawning hidden Alacritty");
            let prewarm = Arc::new(PrewarmManager::new());
            let settings_clone = s.nvim_edit.clone();
            let prewarm_clone = Arc::clone(&prewarm);
            std::thread::spawn(move || {
                prewarm_clone.spawn_prewarm(&settings_clone);
            });
            edit_session_manager.set_prewarm_manager(prewarm);
        }
    }

    let edit_session_manager = Arc::new(edit_session_manager);
    let double_tap_manager = Arc::new(Mutex::new(DoubleTapManager::new()));

    // Create double-tap callback that handles mode activation
    let double_tap_callback = {
        let settings_for_dt = Arc::clone(&settings);
        let edit_session_manager_for_dt = Arc::clone(&edit_session_manager);

        Box::new(move |double_tap_key: DoubleTapKey| {
            handle_double_tap_activation(
                double_tap_key,
                &settings_for_dt,
                &edit_session_manager_for_dt,
            );
        }) as keyboard_handler::DoubleTapCallback
    };

    let keyboard_capture = KeyboardCapture::new();
    keyboard_capture.set_callback(create_keyboard_callback(KeyboardCallbackContext {
        vim_state: Arc::clone(&vim_state),
        settings: Arc::clone(&settings),
        record_key_tx: Arc::clone(&record_key_tx),
        edit_session_manager: Arc::clone(&edit_session_manager),
        double_tap_manager: Arc::clone(&double_tap_manager),
        double_tap_callback,
    }));

    // Set up flags changed callback for double-tap modifier shortcuts
    {
        let settings_for_flags = Arc::clone(&settings);
        let edit_session_manager_for_flags = Arc::clone(&edit_session_manager);
        let double_tap_manager_for_flags = Arc::clone(&double_tap_manager);

        keyboard_capture.set_flags_changed_callback(move |modifiers| {
            let mut dt_manager = double_tap_manager_for_flags.lock().unwrap();

            // Process the flags change and check for double-tap
            if let Some(double_tap_key) = dt_manager.process_flags_changed(
                modifiers.command,
                modifiers.option,
                modifiers.control,
                modifiers.shift,
            ) {
                drop(dt_manager);
                handle_double_tap_activation(
                    double_tap_key,
                    &settings_for_flags,
                    &edit_session_manager_for_flags,
                );
            }
        });
    }

    let app_state = AppState {
        settings,
        vim_state: Arc::clone(&vim_state),
        keyboard_capture,
        record_key_tx,
        edit_session_manager,
    };

    let mode_rx = Arc::new(Mutex::new(mode_rx));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::check_permission,
            commands::get_permission_status,
            commands::complete_permission_setup,
            commands::open_accessibility_settings,
            commands::open_input_monitoring_settings,
            commands::get_vim_mode,
            commands::get_settings,
            commands::set_settings,
            commands::start_capture,
            commands::stop_capture,
            commands::is_capture_running,
            commands::open_settings_window,
            commands::pick_app,
            commands::get_selection_info,
            commands::get_battery_info,
            commands::get_caps_lock_state,
            commands::run_shell_widget,
            commands::get_pending_keys,
            commands::get_key_display_name,
            commands::record_key,
            commands::cancel_record_key,
            commands::webview_log,
            commands::validate_nvim_edit_paths,
            commands::remove_domain_filetype,
            commands::get_domain_filetypes,
            commands::set_indicator_ignores_mouse,
            commands::is_command_key_pressed,
            commands::is_mouse_over_indicator,
            commands::toggle_indicator_visible,
            commands::get_version,
        ])
        .setup(move |app| {
            let show_in_dock = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .show_in_dock;
            if let Err(error) = apply_activation_policy(app.handle(), show_in_dock) {
                log::error!("Failed to apply Dock icon preference: {}", error);
            }

            // Store app handle for global access (used by keyboard handler for events)
            let _ = APP_HANDLE.set(app.handle().clone());

            let initial_indicator_visible = Settings::load().indicator_visible;
            let show_indicator_item = CheckMenuItem::with_id(
                app,
                "show_indicator",
                "Show floating indicator",
                true,
                initial_indicator_visible,
                None::<&str>,
            )?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_indicator_item, &settings_item, &quit_item])?;

            if let Some(tray) = app.tray_by_id("main") {
                tray.set_menu(Some(menu))?;
                tray.on_menu_event(|app, event| match event.id.as_ref() {
                    "show_indicator" => {
                        let state: State<AppState> = app.state();
                        let mut settings = state.settings.lock().unwrap();
                        settings.indicator_visible = !settings.indicator_visible;
                        let _ = settings.save();
                        let new_settings = settings.clone();
                        drop(settings);
                        let _ = app.emit("settings-changed", new_settings);
                    }
                    "settings" => {
                        show_settings_window(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                });

                let initial_settings = Settings::load();
                if let Err(e) = tray.set_visible(initial_settings.show_in_menu_bar) {
                    log::error!("Failed to set initial tray visibility: {}", e);
                }

                let tray_clone = tray.clone();
                let show_indicator_item_clone = show_indicator_item.clone();
                app.listen("settings-changed", move |event| {
                    if let Ok(new_settings) = serde_json::from_str::<Settings>(event.payload()) {
                        if let Err(e) = tray_clone.set_visible(new_settings.show_in_menu_bar) {
                            log::error!("Failed to update tray visibility: {}", e);
                        }
                        // Sync the check menu item with indicator_visible setting
                        let _ = show_indicator_item_clone.set_checked(new_settings.indicator_visible);
                        // Update tray icon when show_mode_in_menu_bar changes
                        update_tray_icon(&tray_clone, "insert", new_settings.show_mode_in_menu_bar);
                    }
                });

                // Listen for mode changes to update tray icon
                let tray_for_mode = tray.clone();
                let app_handle_for_tray = app.handle().clone();
                app.listen("mode-change", move |event| {
                    let mode = event.payload().trim_matches('"');
                    let state: State<AppState> = app_handle_for_tray.state();
                    let show_mode = state.settings.lock().map(|s| s.show_mode_in_menu_bar).unwrap_or(false);
                    update_tray_icon(&tray_for_mode, mode, show_mode);
                });
            }

            if let Some(indicator_window) = app.get_webview_window("indicator") {
                if let Err(e) = setup_indicator_window(&indicator_window) {
                    log::error!("Failed to setup indicator window: {}", e);
                }
            }

            if let Some(settings_window) = app.get_webview_window("settings") {
                let window = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                });
            }

            if let Some(permissions_window) = app.get_webview_window("permissions") {
                let window = permissions_window.clone();
                permissions_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        crate::window::hide_permission_drag_helper();
                        let _ = window.hide();
                    }
                });
            }

            let app_handle = app.handle().clone();
            let mut rx = mode_rx.lock().unwrap().resubscribe();

            tauri::async_runtime::spawn(async move {
                while let Ok(mode) = rx.recv().await {
                    log::info!("Mode changed to: {:?}", mode);
                    let _ = app_handle.emit("mode-change", mode.as_str());
                }
            });

            if check_accessibility_permission() && check_input_monitoring_permission() {
                let state: State<AppState> = app.state();
                if let Err(e) = state.keyboard_capture.start() {
                    log::error!("Failed to start keyboard capture: {}", e);
                } else {
                    log::info!("Keyboard capture started automatically");
                }
            } else {
                log::warn!("Required macOS permissions are not granted");
                if let Some(window) = app.get_webview_window("permissions") {
                    if let Err(error) = window.show() {
                        log::error!("Failed to show permission setup window: {}", error);
                    } else if let Err(error) = window.set_focus() {
                        log::error!("Failed to focus permission setup window: {}", error);
                    }
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            log::info!("Application reopened - showing settings");
            show_settings_window(app);
        }
    });
}
