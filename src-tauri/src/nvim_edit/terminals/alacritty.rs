//! Alacritty terminal spawner

use std::path::Path;
use std::process::Command;

use super::process_utils::{find_editor_pid_for_file, resolve_command_path, resolve_terminal_path};
use super::{EditorContext, SpawnInfo, TerminalSpawner, TerminalType, WindowGeometry};
use crate::config::NvimEditSettings;


pub struct AlacrittySpawner;

/// Configuration for spawning an Alacritty window
struct SpawnConfig {
    title: String,
    columns: u32,
    lines: u32,
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    editor_cmd: Vec<String>,
    terminal_path: String,
}

impl SpawnConfig {
    fn new(settings: &NvimEditSettings, file_path: &str, socket_path: Option<&Path>, text_is_empty: bool, filetype: Option<&str>) -> Self {
        let editor_path = settings.editor_path();
        let resolved_editor = resolve_command_path(&editor_path);
        log::info!("Resolved editor path: {} -> {}", editor_path, resolved_editor);

        let terminal_cmd = settings.get_terminal_path();
        let terminal_path = resolve_terminal_path(&terminal_cmd);
        log::info!("Resolved terminal path: {} -> {}", terminal_cmd, terminal_path);

        // Build editor command with all args
        let mut editor_cmd = vec![resolved_editor.clone()];

        // Add socket args for nvim RPC if applicable
        if let Some(socket) = socket_path {
            if editor_path.contains("nvim") || editor_path == "nvim" {
                editor_cmd.push("--listen".to_string());
                editor_cmd.push(socket.to_string_lossy().to_string());
            }
        }

        // Add filetype command if provided (for nvim/vim)
        if let Some(ft) = filetype {
            if editor_path.contains("nvim") || editor_path.contains("vim") {
                editor_cmd.push("-c".to_string());
                editor_cmd.push(format!("set ft={}", ft));
            }
        }

        // Add editor args from settings (insert mode if text is empty)
        for arg in settings.editor_args(text_is_empty) {
            editor_cmd.push(arg.to_string());
        }

        // Add file path
        editor_cmd.push(file_path.to_string());

        Self {
            title: format!("ovim-edit-{}", std::process::id()),
            columns: 80,
            lines: 24,
            x: None,
            y: None,
            width: None,
            height: None,
            editor_cmd,
            terminal_path,
        }
    }

    fn with_geometry(mut self, geometry: Option<&WindowGeometry>) -> Self {
        if let Some(geo) = geometry {
            self.columns = (geo.width / 8).max(40);
            self.lines = (geo.height / 16).max(10);
            self.x = Some(geo.x);
            self.y = Some(geo.y);
            self.width = Some(geo.width);
            self.height = Some(geo.height);
        }
        self
    }
}

impl TerminalSpawner for AlacrittySpawner {
    fn terminal_type(&self) -> TerminalType {
        TerminalType::Alacritty
    }

    fn spawn(
        &self,
        settings: &NvimEditSettings,
        file_path: &str,
        geometry: Option<WindowGeometry>,
        socket_path: Option<&Path>,
        editor_context: EditorContext<'_>,
    ) -> Result<SpawnInfo, String> {
        let EditorContext { text_is_empty, filetype } = editor_context;
        let config = SpawnConfig::new(settings, file_path, socket_path, text_is_empty, filetype)
            .with_geometry(geometry.as_ref());

        // Reuse the running application, including positioned popups.
        let child = self.spawn_window(&config)?;

        // Find editor PID
        let pid = find_editor_pid_for_file(file_path, settings.editor_process_name());
        log::info!("Found editor PID: {:?} for file: {}", pid, file_path);

        if settings.popup_mode {
            // Browser accessibility can omit geometry; this is still a popup.
            super::aerospace::float_window(&config.title);
        }
        if let Some(geo) = geometry.as_ref() {
            super::applescript_utils::set_window_bounds_by_title(
                &["alacritty", "Alacritty"], &config.title,
                geo.x, geo.y, geo.width, geo.height,
            );
        }

        Ok(SpawnInfo {
            terminal_type: TerminalType::Alacritty,
            process_id: pid,
            child,
            window_title: Some(config.title),
        })
    }
}

impl AlacrittySpawner {
    /// Reuse Alacritty's IPC server; launch a process only when IPC is unavailable.
    fn spawn_window(&self, config: &SpawnConfig) -> Result<Option<std::process::Child>, String> {
        let args = self.build_msg_args(config);
        match Command::new(&config.terminal_path).args(&args).status() {
            Ok(status) if status.success() => {
                log::info!("msg create-window succeeded");
                Ok(None)
            }
            _ => {
                log::info!("msg create-window failed, falling back to regular spawn");
                self.spawn_new_process(config).map(Some)
            }
        }
    }

    /// Build args for msg create-window command
    fn build_msg_args(&self, config: &SpawnConfig) -> Vec<String> {
        let mut args = vec![
            "msg".to_string(),
            "create-window".to_string(),
        ];

        args.extend(self.window_options(config));

        args.push("-e".to_string());
        args.extend(config.editor_cmd.clone());
        args
    }

    /// Spawn a new Alacritty process directly
    fn spawn_new_process(&self, config: &SpawnConfig) -> Result<std::process::Child, String> {
        let mut args = self.window_options(config);
        args.push("-e".to_string());
        args.extend(config.editor_cmd.clone());

        Command::new(&config.terminal_path)
            .args(&args)
            .spawn()
            .map_err(|e| format!("Failed to spawn alacritty: {}", e))
    }

    /// Common window options for Alacritty
    fn window_options(&self, config: &SpawnConfig) -> Vec<String> {
        let mut args = vec![
            "-o".to_string(),
            format!("window.title=\"{}\"", config.title),
            "-o".to_string(),
            "window.dynamic_title=false".to_string(),
            "-o".to_string(),
            "window.startup_mode=\"Windowed\"".to_string(),
            "-o".to_string(),
            format!("window.dimensions.columns={}", config.columns),
            "-o".to_string(),
            format!("window.dimensions.lines={}", config.lines),
        ];

        // Add position if available - this positions the window at spawn time
        // avoiding the slow AppleScript animation.
        // Must use object syntax to set both x and y together, otherwise only one axis applies.
        // See: https://github.com/alacritty/alacritty/issues/7518
        // Note: Alacritty uses physical pixels, but our coordinates are in points (macOS).
        // On Retina displays, we need to multiply by the scale factor (typically 2).
        if let (Some(x), Some(y)) = (config.x, config.y) {
            // TODO: Get actual scale factor instead of assuming 2x
            let scale = 2;
            let pos_opt = format!("window.position={{x={},y={}}}", x * scale, y * scale);
            log::info!("Alacritty position option: {}", pos_opt);
            args.push("-o".to_string());
            args.push(pos_opt);
        }

        args
    }
}

/// Result of spawning a pre-warmed Alacritty window
pub struct PrewarmSpawnResult {
    pub process_id: Option<u32>,
    pub child: Option<std::process::Child>,
    pub window_title: String,
}

impl AlacrittySpawner {
    /// Spawn a hidden, off-screen Alacritty window with nvim listening on a socket.
    /// No file is loaded yet - nvim starts with an empty buffer.
    pub fn spawn_prewarm_window(
        settings: &NvimEditSettings,
        socket_path: &std::path::Path,
    ) -> Result<PrewarmSpawnResult, String> {
        let editor_path = settings.editor_path();
        let resolved_editor = resolve_command_path(&editor_path);
        let terminal_cmd = settings.get_terminal_path();
        let terminal_path = resolve_terminal_path(&terminal_cmd);

        let title = format!("ovim-prewarm-{}", std::process::id());

        // Build editor command: nvim --listen <socket> (no file)
        let editor_cmd = vec![
            resolved_editor,
            "--listen".to_string(),
            socket_path.to_string_lossy().to_string(),
        ];

        let config = SpawnConfig {
            title: title.clone(),
            columns: 80,
            lines: 24,
            x: Some(-10000),
            y: Some(-10000),
            width: Some(640),
            height: Some(384),
            editor_cmd,
            terminal_path,
        };
        let child = Self.spawn_window(&config)?;
        let child_pid = child.as_ref().map(std::process::Child::id);

        // Wait for nvim to start and create the socket
        let mut waited = 0;
        while waited < 5000 {
            if socket_path.exists() {
                log::info!("Prewarm nvim socket ready after {}ms", waited);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            waited += 50;
        }

        if !socket_path.exists() {
            log::warn!("Prewarm nvim socket not found after 5s, continuing anyway");
        }

        if super::aerospace::float_window(&title) {
            super::applescript_utils::set_window_bounds_by_title(
                &["alacritty", "Alacritty"], &title, -10000, -10000, 640, 384,
            );
        }

        // Find the nvim PID inside the terminal
        let nvim_pid = find_nvim_pid_for_socket(socket_path);

        Ok(PrewarmSpawnResult {
            process_id: nvim_pid.or(child_pid),
            child,
            window_title: title,
        })
    }
}

/// Find the nvim process listening on a given socket
fn find_nvim_pid_for_socket(socket_path: &std::path::Path) -> Option<u32> {
    let socket_str = socket_path.to_string_lossy();
    let output = Command::new("pgrep")
        .args(["-f", &format!("nvim.*--listen.*{}", socket_str)])
        .output()
        .ok()?;
    if output.status.success() {
        let pid_str = String::from_utf8_lossy(&output.stdout);
        // Take first matching PID
        pid_str.lines().next()?.trim().parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positioned_popup_reuses_successful_ipc_without_another_process() {
        let config = SpawnConfig {
            title: "ovim-edit-test".to_string(),
            columns: 80,
            lines: 24,
            x: Some(100),
            y: Some(200),
            width: Some(640),
            height: Some(384),
            editor_cmd: vec!["nvim".to_string()],
            terminal_path: "/usr/bin/true".to_string(),
        };
        assert!(AlacrittySpawner.spawn_window(&config).expect("IPC success").is_none());
        let args = AlacrittySpawner.build_msg_args(&config);
        assert_eq!(&args[..2], &["msg", "create-window"]);
        assert!(args.iter().any(|arg| arg == "window.position={x=200,y=400}"));
    }
}
