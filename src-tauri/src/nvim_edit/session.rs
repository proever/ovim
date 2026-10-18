//! Edit session management for "Edit with Neovim" feature

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use uuid::Uuid;

use super::accessibility::FocusContext;
use super::prewarm::PrewarmManager;
use super::terminals::{spawn_terminal, WindowGeometry};
use crate::config::NvimEditSettings;

/// An active edit session
#[derive(Clone)]
pub struct EditSession {
    pub focus_context: FocusContext,
    pub temp_file: PathBuf,
    pub file_mtime: SystemTime,
    pub process_id: Option<u32>,
    /// Socket path for RPC communication with nvim
    pub socket_path: PathBuf,
    /// Domain key for filetype persistence (browser hostname or app bundle ID)
    pub domain_key: String,
}

/// Manager for edit sessions
pub struct EditSessionManager {
    sessions: Arc<Mutex<HashMap<Uuid, EditSession>>>,
    prewarm: Option<Arc<PrewarmManager>>,
}

impl EditSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            prewarm: None,
        }
    }

    /// Set the prewarm manager (called after construction)
    pub fn set_prewarm_manager(&mut self, prewarm: Arc<PrewarmManager>) {
        self.prewarm = Some(prewarm);
    }

    /// Start a new edit session
    pub fn start_session(
        &self,
        focus_context: FocusContext,
        text: &str,
        settings: NvimEditSettings,
        geometry: Option<WindowGeometry>,
        domain_key: String,
        saved_filetype: Option<&str>,
    ) -> Result<Uuid, String> {
        // Create temp directory if needed
        let cache_dir = dirs::cache_dir()
            .ok_or("Could not determine cache directory")?
            .join("ovim");
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;

        // Generate session ID and temp file
        let session_id = Uuid::new_v4();
        let temp_file = cache_dir.join(format!("edit_{}.txt", session_id));

        // Generate socket path for RPC
        let socket_path = cache_dir.join(format!("nvim_{}.sock", session_id));

        // Clean up any stale socket file
        let _ = std::fs::remove_file(&socket_path);

        // Write text to temp file
        std::fs::write(&temp_file, text)
            .map_err(|e| format!("Failed to write temp file: {}", e))?;

        // Get file modification time after writing
        let file_mtime = std::fs::metadata(&temp_file)
            .and_then(|m| m.modified())
            .map_err(|e| format!("Failed to get file mtime: {}", e))?;

        // Consider whitespace-only text as empty (start in insert mode)
        let text_is_empty = text.trim().is_empty();

        // Try the pre-warmed terminal path first
        let process_id =
            if let Some(ref prewarm) = self.prewarm {
                if let Some((prewarm_socket, prewarm_pid, prewarm_title)) = prewarm.try_claim() {
                    log::info!("Using pre-warmed terminal: {}", prewarm_title);

                    // Load the file into the pre-warmed nvim via RPC
                    match super::prewarm::load_file_via_rpc(
                        &prewarm_socket,
                        &temp_file,
                        saved_filetype,
                        text_is_empty,
                    ) {
                        Ok(()) => {
                            log::info!("File loaded into pre-warmed nvim");

                            // Reposition and show the window
                            if let Some(ref geo) = geometry {
                                super::prewarm::show_and_position(&prewarm_title, geo);
                            } else {
                                // No geometry - just focus the window via AppleScript
                                super::terminals::applescript_utils::focus_window_by_title(
                                    &["alacritty", "Alacritty"],
                                    &prewarm_title,
                                );
                            }

                            // The socket is now the prewarm socket (nvim is already listening)
                            // We need to update socket_path to point to the prewarm socket
                            // so the RPC handler can connect to it
                            let _ = std::fs::remove_file(&socket_path);
                            // Copy the prewarm socket path to our session socket path
                            // Actually, just use the prewarm socket directly
                            let actual_socket = prewarm_socket;

                            // Schedule respawn in background
                            let prewarm_clone = Arc::clone(prewarm);
                            let settings_clone = settings.clone();
                            std::thread::spawn(move || {
                                prewarm_clone.schedule_respawn(settings_clone);
                            });

                            // Override socket_path for this session
                            let session = EditSession {
                                focus_context,
                                temp_file,
                                file_mtime,
                                process_id: prewarm_pid,
                                socket_path: actual_socket,
                                domain_key,
                            };

                            let mut sessions = self.sessions.lock().unwrap();
                            sessions.insert(session_id, session);
                            return Ok(session_id);
                        }
                        Err(e) => {
                            log::warn!("Failed to load file into pre-warmed nvim: {}, falling back to normal spawn", e);
                            // Fall through to normal spawn
                        }
                    }
                }
                // Prewarm not available, fall through
                self.normal_spawn(&settings, &temp_file, geometry, &socket_path, text_is_empty, saved_filetype)?
            } else {
                self.normal_spawn(&settings, &temp_file, geometry, &socket_path, text_is_empty, saved_filetype)?
            };

        // Create session
        let session = EditSession {
            focus_context,
            temp_file,
            file_mtime,
            process_id,
            socket_path,
            domain_key,
        };

        // Store session
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session_id, session);

        Ok(session_id)
    }

    /// Normal terminal spawn (non-prewarm path), returns the editor PID if found
    fn normal_spawn(
        &self,
        settings: &NvimEditSettings,
        temp_file: &std::path::Path,
        geometry: Option<WindowGeometry>,
        socket_path: &std::path::Path,
        text_is_empty: bool,
        saved_filetype: Option<&str>,
    ) -> Result<Option<u32>, String> {
        let spawn_info = spawn_terminal(settings, temp_file, geometry, Some(socket_path), text_is_empty, saved_filetype)?;
        Ok(spawn_info.process_id)
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &Uuid) -> Option<EditSession> {
        self.sessions.lock().unwrap().get(id).cloned()
    }

    /// Remove a session after completion
    pub fn remove_session(&self, id: &Uuid) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.remove(id);
    }
}

impl Default for EditSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
