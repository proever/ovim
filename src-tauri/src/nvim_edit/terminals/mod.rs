//! Terminal spawning for Edit Popup feature
//!
//! This module provides a unified interface for spawning different terminal emulators
//! with various text editors (Neovim, Vim, Helix, etc.)

mod alacritty;
pub mod applescript_utils;
mod ghostty;
mod iterm;
mod kitty;
pub mod process_utils;
mod terminal_app;
mod wezterm;

pub use alacritty::AlacrittySpawner;
pub use ghostty::GhosttySpawner;
pub use iterm::ITermSpawner;
pub use kitty::KittySpawner;
pub use terminal_app::TerminalAppSpawner;
pub use wezterm::WezTermSpawner;

use crate::config::NvimEditSettings;
use std::path::Path;

/// Window position and size for popup mode
#[derive(Debug, Clone, Default)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Terminal types supported
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    Alacritty,
    Ghostty,
    Kitty,
    WezTerm,
    ITerm,
    Default, // Terminal.app
}

impl TerminalType {
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "alacritty" => TerminalType::Alacritty,
            "ghostty" => TerminalType::Ghostty,
            "kitty" => TerminalType::Kitty,
            "wezterm" => TerminalType::WezTerm,
            "iterm" | "iterm2" => TerminalType::ITerm,
            _ => TerminalType::Default,
        }
    }
}

/// Spawn info returned after launching terminal
pub struct SpawnInfo {
    /// PID of the editor (or of a terminal process that exits with it)
    pub process_id: Option<u32>,
}

/// Editor state that influences terminal startup behavior.
#[derive(Clone, Copy)]
pub struct EditorContext<'a> {
    pub text_is_empty: bool,
    pub filetype: Option<&'a str>,
}

/// Trait for terminal spawners
pub trait TerminalSpawner {
    /// Spawn a terminal with the configured editor editing the given file
    ///
    /// If `socket_path` is provided, the editor will be started with RPC enabled
    /// (e.g., nvim --listen <socket_path>) so ovim can sync the cursor and filetype.
    ///
    /// If `text_is_empty` is true, the editor should start in insert mode.
    ///
    /// If `filetype` is provided, the editor will set the filetype on startup
    /// (e.g., nvim -c "set ft=markdown").
    fn spawn(
        &self,
        settings: &NvimEditSettings,
        file_path: &str,
        geometry: Option<WindowGeometry>,
        socket_path: Option<&Path>,
        editor_context: EditorContext<'_>,
    ) -> Result<SpawnInfo, String>;
}

/// Spawn a terminal with the configured editor editing the given file
///
/// If `socket_path` is provided, the editor will be started with RPC enabled
/// so ovim can sync the cursor and filetype.
///
/// If `text_is_empty` is true, the editor should start in insert mode.
///
/// If `filetype` is provided, the editor will set the filetype on startup.
pub fn spawn_terminal(
    settings: &NvimEditSettings,
    temp_file: &Path,
    geometry: Option<WindowGeometry>,
    socket_path: Option<&Path>,
    text_is_empty: bool,
    filetype: Option<&str>,
) -> Result<SpawnInfo, String> {
    let terminal_type = TerminalType::from_string(&settings.terminal);
    let file_path = temp_file.to_string_lossy();

    let editor_context = EditorContext { text_is_empty, filetype };
    match terminal_type {
        TerminalType::Alacritty => AlacrittySpawner.spawn(settings, &file_path, geometry, socket_path, editor_context),
        TerminalType::Ghostty => GhosttySpawner.spawn(settings, &file_path, geometry, socket_path, editor_context),
        TerminalType::Kitty => KittySpawner.spawn(settings, &file_path, geometry, socket_path, editor_context),
        TerminalType::WezTerm => WezTermSpawner.spawn(settings, &file_path, geometry, socket_path, editor_context),
        TerminalType::ITerm => ITermSpawner.spawn(settings, &file_path, geometry, socket_path, editor_context),
        TerminalType::Default => TerminalAppSpawner.spawn(settings, &file_path, geometry, socket_path, editor_context),
    }
}
