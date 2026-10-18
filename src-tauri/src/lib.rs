// Allow unexpected_cfgs from the objc crate's macros which use cfg(feature = "cargo-clippy")
#![allow(unexpected_cfgs)]

mod commands;
mod config;
mod keyboard;
mod nvim_edit;
mod shortcut;
mod window;

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{
    menu::{Menu, MenuItem},
    AppHandle, Manager,
};

use config::Settings;
use keyboard::check_accessibility_permission;
use nvim_edit::prewarm::PrewarmManager;
use nvim_edit::EditSessionManager;

static LOG_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

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
    pub edit_session_manager: Arc<EditSessionManager>,
}

fn show_window(app: &AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        log::error!("Window '{}' is unavailable", label);
        return;
    };

    if let Err(error) = window.show() {
        log::error!("Failed to show window '{}': {}", label, error);
        return;
    }

    if let Err(error) = window.set_focus() {
        log::error!("Failed to focus window '{}': {}", label, error);
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

    let settings = Settings::load();
    let mut edit_session_manager = EditSessionManager::new();

    // Initialize prewarm manager if enabled and using alacritty
    if settings.nvim_edit.prewarm_terminal && settings.nvim_edit.terminal == "alacritty" {
        log::info!("Pre-warming terminal enabled, spawning hidden Alacritty");
        let prewarm = Arc::new(PrewarmManager::new());
        let settings_clone = settings.nvim_edit.clone();
        let prewarm_clone = Arc::clone(&prewarm);
        std::thread::spawn(move || {
            prewarm_clone.spawn_prewarm(&settings_clone);
        });
        edit_session_manager.set_prewarm_manager(prewarm);
    }

    let app_state = AppState {
        settings: Arc::new(Mutex::new(settings)),
        edit_session_manager: Arc::new(edit_session_manager),
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(shortcut::plugin())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_permission_status,
            commands::complete_permission_setup,
            commands::open_accessibility_settings,
            commands::get_settings,
            commands::set_settings,
            commands::get_version,
            commands::get_key_display_name,
            commands::shortcut_key_from_code,
            commands::set_shortcut_paused,
            commands::get_shortcut_error,
            commands::validate_nvim_edit_paths,
            commands::remove_domain_filetype,
            commands::get_domain_filetypes,
        ])
        .setup(|app| {
            let settings = app.state::<AppState>().settings.lock().unwrap().clone();

            if let Err(error) = apply_activation_policy(app.handle(), settings.show_in_dock) {
                log::error!("Failed to apply Dock icon preference: {}", error);
            }

            shortcut::register(app.handle(), &settings.nvim_edit);

            let settings_item =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            if let Some(tray) = app.tray_by_id("main") {
                tray.set_menu(Some(menu))?;
                tray.on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_window(app, "settings"),
                    "quit" => app.exit(0),
                    _ => {}
                });

                if let Err(e) = tray.set_visible(settings.show_in_menu_bar) {
                    log::error!("Failed to set initial tray visibility: {}", e);
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

            if !check_accessibility_permission() {
                log::warn!("Accessibility permission is not granted");
                show_window(app.handle(), "permissions");
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            log::info!("Application reopened - showing settings");
            show_window(app, "settings");
        }
    });
}
