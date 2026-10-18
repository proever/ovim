//! "Edit with Neovim" feature - open any text field in nvim via a keyboard shortcut

pub mod accessibility;
mod browser_scripting;
mod clipboard;
mod geometry;
pub mod prewarm;
mod rpc;
mod session;
pub mod terminals;
mod text_capture;

pub use session::EditSessionManager;

use crate::config::{NvimEditSettings, Settings};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Trigger the "Edit with Neovim" flow
/// `shared_settings` receives the filetype nvim ends up with, so it is remembered for the field
pub fn trigger_nvim_edit(
    manager: Arc<EditSessionManager>,
    settings: NvimEditSettings,
    shared_settings: Arc<Mutex<Settings>>,
) -> Result<(), String> {
    // 1. Capture focus context (which app we're in)
    let focus_context =
        accessibility::capture_focus_context().ok_or("No focused application found")?;
    log::info!("Captured focus context: {:?}", focus_context);

    // 2. Capture geometry info BEFORE any clipboard operations (which may change focus)
    log::info!(
        "popup_mode={}, popup_width={}, popup_height={}",
        settings.popup_mode,
        settings.popup_width,
        settings.popup_height
    );
    let element_frame = accessibility::get_focused_element_frame();
    let window_frame = accessibility::get_focused_window_frame();
    log::info!(
        "Element frame from accessibility: {:?}",
        element_frame
            .as_ref()
            .map(|f| (f.x, f.y, f.width, f.height))
    );
    log::info!(
        "Window frame: {:?}",
        window_frame.as_ref().map(|f| (f.x, f.y, f.width, f.height))
    );

    // 3. Capture text and get element frame (may use browser scripting as fallback)
    let capture_result = text_capture::capture_text_and_frame(
        &focus_context.app_bundle_id,
        element_frame,
        settings.clipboard_mode,
    );
    let text = capture_result.text;
    let element_frame = capture_result.element_frame;
    let initial_cursor = capture_result.cursor_position;
    let browser_target = capture_result.browser_target;

    if let Some(ref cursor) = initial_cursor {
        log::info!(
            "Initial cursor position: line={}, col={}",
            cursor.line,
            cursor.column
        );
    }

    // 4. Determine domain key for filetype persistence
    // For browsers, use the hostname. For native apps, use bundle ID.
    let domain_key = if let Some(target) = browser_target {
        browser_scripting::get_browser_hostname(target)
            .unwrap_or_else(|| focus_context.app_bundle_id.clone())
    } else {
        focus_context.app_bundle_id.clone()
    };
    log::info!("Domain key for filetype: {}", domain_key);

    // 5. Look up saved filetype for this domain
    let saved_filetype = settings
        .get_filetype_for_domain(&domain_key)
        .map(|s| s.to_string());
    if let Some(ref ft) = saved_filetype {
        log::info!("Found saved filetype for domain '{}': {}", domain_key, ft);
    }

    // 6. Calculate window geometry if popup mode is enabled
    let geometry = geometry::calculate_popup_geometry(&settings, element_frame, window_frame);
    log::info!("Final geometry: {:?}", geometry);

    // 7. Start edit session (writes temp file, spawns terminal)
    let session_id = manager.start_session(
        focus_context,
        &text,
        settings.clone(),
        geometry,
        domain_key,
        saved_filetype.as_deref(),
    )?;
    log::info!("Started edit session: {}", session_id);

    // 8. Watch the editor in the background until it exits
    let session = manager
        .get_session(&session_id)
        .ok_or("Session not found immediately after creation")?;

    let monitor = if settings.clipboard_mode {
        log::info!("Clipboard mode enabled, waiting for the editor without RPC");
        let process_id = session.process_id;
        thread::spawn(move || {
            wait_for_editor_exit(process_id);
            None
        })
    } else {
        spawn_editor_monitor(&session, initial_cursor)
    };

    // 9. Spawn main thread to wait for nvim to exit and restore text
    spawn_completion_handler(manager, session_id, monitor, browser_target, shared_settings);

    Ok(())
}

/// Cursor position and filetype nvim reported before it exited
struct EditorState {
    final_cursor: Option<browser_scripting::CursorPosition>,
    filetype: Option<String>,
}

/// Check if the editor process is still running
fn editor_process_exists(pid: Option<u32>) -> bool {
    if let Some(pid) = pid {
        // Check if process exists by sending signal 0
        unsafe { libc::kill(pid as i32, 0) == 0 }
    } else {
        true // Can't check without PID, assume exists
    }
}

/// Wait for editor process to exit (used when there is no RPC connection)
fn wait_for_editor_exit(process_id: Option<u32>) {
    loop {
        if !editor_process_exists(process_id) {
            log::info!("Editor process exited");
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Spawn a thread that follows the editor until it exits.
///
/// Neovim is followed over RPC, which also places its cursor and reports the
/// final cursor and filetype. Editors that cannot be reached over RPC are
/// followed by process ID.
fn spawn_editor_monitor(
    session: &session::EditSession,
    initial_cursor: Option<browser_scripting::CursorPosition>,
) -> thread::JoinHandle<Option<EditorState>> {
    let socket_path = session.socket_path.clone();
    let process_id = session.process_id;

    thread::spawn(move || {
        let state = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt.block_on(monitor_over_rpc(&socket_path, process_id, initial_cursor)),
            Err(e) => {
                log::error!("Failed to create tokio runtime: {}", e);
                None
            }
        };

        if state.is_none() {
            wait_for_editor_exit(process_id);
        }
        state
    })
}

/// Follow nvim over RPC until it exits. Returns None if nvim cannot be reached.
async fn monitor_over_rpc(
    socket_path: &Path,
    process_id: Option<u32>,
    initial_cursor: Option<browser_scripting::CursorPosition>,
) -> Option<EditorState> {
    log::info!("Attempting RPC connection to {:?}", socket_path);

    let rpc_session = match rpc::connect_to_nvim(socket_path).await {
        Ok(rpc_session) => rpc_session,
        Err(e) => {
            log::warn!("RPC connection failed, waiting for the editor process instead: {}", e);
            return None;
        }
    };
    log::info!("RPC connected");

    // Set nvim cursor to match browser's initial cursor position
    if let Some(cursor) = initial_cursor {
        if let Err(e) = rpc_session.set_cursor(cursor.line, cursor.column).await {
            log::debug!("Failed to set initial nvim cursor: {}", e);
        } else {
            log::info!(
                "Set nvim cursor to line={}, col={}",
                cursor.line,
                cursor.column
            );
        }
    }

    // Poll cursor position and filetype periodically until window closes or socket removed
    // Check window first (faster for Cmd+W), then socket (faster for :q)
    let mut last_cursor: Option<browser_scripting::CursorPosition> = None;
    let mut last_filetype: Option<String> = None;
    loop {
        // Try to get current cursor position first
        match rpc_session.get_cursor().await {
            Ok((line, col)) => {
                last_cursor = Some(browser_scripting::CursorPosition { line, column: col });
            }
            Err(_) => {
                // RPC failed, nvim is probably closing
                log::info!("RPC get_cursor failed, nvim closing");
                break;
            }
        }

        // Also get filetype during polling (so we capture it before nvim closes)
        if let Ok(ft) = rpc_session.get_filetype().await {
            if !ft.is_empty() && ft != "text" {
                last_filetype = Some(ft);
            }
        }

        // Check if editor process is gone (fast for Cmd+W close)
        if !editor_process_exists(process_id) {
            log::info!("Editor process exited");
            break;
        }

        // Check socket (fast for :q/:wq)
        if !rpc::socket_exists(socket_path) {
            log::info!("Socket removed, nvim has exited");
            break;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if let Some(ref cursor) = last_cursor {
        log::info!(
            "Final nvim cursor: line={}, col={}",
            cursor.line,
            cursor.column
        );
    }
    if let Some(ref ft) = last_filetype {
        log::info!("Final filetype: {}", ft);
    }

    Some(EditorState {
        final_cursor: last_cursor,
        filetype: last_filetype,
    })
}

/// Spawn the completion handler thread that waits for nvim and restores text
fn spawn_completion_handler(
    manager: Arc<EditSessionManager>,
    session_id: uuid::Uuid,
    monitor: thread::JoinHandle<Option<EditorState>>,
    browser_target: Option<browser_scripting::BrowserType>,
    shared_settings: Arc<Mutex<Settings>>,
) {
    thread::spawn(move || {
        let Some(session) = manager.get_session(&session_id) else {
            log::error!("Session not found: {}", session_id);
            return;
        };

        // The monitor detects nvim exit via socket removal, which is faster
        // than waiting for the process to exit on Cmd+W window close
        log::info!("Waiting for the editor to exit");
        let editor_state = monitor.join().ok().flatten();
        let final_cursor = editor_state.as_ref().and_then(|s| s.final_cursor);
        let final_filetype = editor_state.and_then(|s| s.filetype);

        // Save the filetype for this domain if we got one
        if let Some(ref ft) = final_filetype {
            log::info!(
                "Saving filetype '{}' for domain '{}'",
                ft,
                session.domain_key
            );

            shared_settings
                .lock()
                .unwrap()
                .nvim_edit
                .set_filetype_for_domain(session.domain_key.clone(), ft.clone());
        }

        log::info!("Nvim exited, restoring focus");

        // Small delay to let the system settle after window close
        thread::sleep(Duration::from_millis(50));

        // Restore focus to the original app, with retry
        for attempt in 0..3 {
            match accessibility::restore_focus(&session.focus_context) {
                Ok(()) => break,
                Err(e) => {
                    if attempt < 2 {
                        log::info!("Retry {} restoring focus: {}", attempt + 1, e);
                        thread::sleep(Duration::from_millis(50));
                    } else {
                        log::error!("Error restoring focus after retries: {}", e);
                    }
                }
            }
        }

        let text_replaced = match complete_edit_session(&manager, &session_id) {
            Ok(replaced) => replaced,
            Err(e) => {
                log::error!("Error completing edit session: {}", e);
                false
            }
        };

        // Move the browser cursor to where it was in nvim
        if text_replaced {
            if let (Some(target), Some(cursor)) = (browser_target, final_cursor) {
                log::info!(
                    "Restoring browser cursor to line={}, col={}",
                    cursor.line,
                    cursor.column
                );
                match browser_scripting::set_browser_cursor_position(
                    target,
                    cursor.line,
                    cursor.column,
                ) {
                    Ok(()) => log::info!("Browser cursor restored successfully"),
                    Err(e) => log::info!("Failed to restore browser cursor: {}", e),
                }
            }
        }

        // Clean up socket file
        let _ = std::fs::remove_file(&session.socket_path);

        // Clean up session
        manager.remove_session(&session_id);
    });
}

/// Paste the saved text back into the field via the clipboard and clean up
/// the temp file. Returns whether the field was replaced: quitting nvim
/// without saving leaves the field untouched.
fn complete_edit_session(
    manager: &EditSessionManager,
    session_id: &uuid::Uuid,
) -> Result<bool, String> {
    let session = manager.get_session(session_id).ok_or("Session not found")?;

    log::info!("Reading temp file: {:?}", session.temp_file);

    // Debug: write to temp file for troubleshooting
    let debug_log = |msg: &str| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/ovim_debug.log")
        {
            let _ = writeln!(
                f,
                "{}: {}",
                chrono::Local::now().format("%H:%M:%S%.3f"),
                msg
            );
        }
        log::info!("{}", msg);
    };

    // Check if file was modified by comparing modification times
    let current_mtime = std::fs::metadata(&session.temp_file)
        .and_then(|m| m.modified())
        .map_err(|e| format!("Failed to get current file mtime: {}", e))?;

    if current_mtime == session.file_mtime {
        debug_log("File not modified (nvim quit without saving), skipping restoration");
        let _ = std::fs::remove_file(&session.temp_file);
        return Ok(false);
    }

    let edited_text = std::fs::read_to_string(&session.temp_file)
        .map_err(|e| format!("Failed to read temp file: {}", e))?;

    // Strip trailing newline that nvim adds (fixeol option)
    let edited_text = edited_text
        .strip_suffix('\n')
        .unwrap_or(&edited_text)
        .to_string();

    debug_log(&format!("Read {} chars from temp file", edited_text.len()));

    // Clean up temp file
    let _ = std::fs::remove_file(&session.temp_file);

    // Longer delay for focus to settle - browsers like Chrome need more time
    debug_log("Waiting 300ms for focus to settle...");
    thread::sleep(Duration::from_millis(300));

    debug_log(&format!(
        "Replacing text via clipboard, {} chars",
        edited_text.len()
    ));
    clipboard::replace_text_via_clipboard(&edited_text)?;

    debug_log("Successfully restored edited text");
    Ok(true)
}
