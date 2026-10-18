use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::nvim_edit::NvimEditSettings;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Launch at login
    pub launch_at_login: bool,
    /// Show in menu bar
    pub show_in_menu_bar: bool,
    /// Show the application icon in the macOS Dock
    pub show_in_dock: bool,
    /// Settings for Edit Popup feature
    pub nvim_edit: NvimEditSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            show_in_menu_bar: true,
            show_in_dock: false,
            nvim_edit: NvimEditSettings::default(),
        }
    }
}

impl Settings {
    /// Get the path to the YAML settings file
    pub fn file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("ovim").join("settings.yaml"))
    }

    /// Get the path to the legacy JSON settings file
    fn legacy_json_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("ovim").join("settings.json"))
    }

    /// Load settings from disk (YAML format, with JSON migration)
    pub fn load() -> Self {
        let mut settings = Self::load_raw();
        // Sanitize settings to fix any invalid state
        settings.nvim_edit.sanitize();
        // Load domain filetypes from separate file
        settings.nvim_edit.load_domain_filetypes();
        settings
    }

    /// Load raw settings without sanitization
    fn load_raw() -> Self {
        // First, try to load from YAML
        if let Some(yaml_path) = Self::file_path() {
            if let Ok(contents) = std::fs::read_to_string(&yaml_path) {
                if let Ok(settings) = serde_yml::from_str(&contents) {
                    return settings;
                }
            }
        }

        // If no YAML exists, try to migrate from JSON
        if let Some(json_path) = Self::legacy_json_path() {
            if let Ok(contents) = std::fs::read_to_string(&json_path) {
                if let Ok(settings) = serde_json::from_str::<Settings>(&contents) {
                    // Save as YAML and delete the old JSON file
                    if settings.save().is_ok() {
                        let _ = std::fs::remove_file(&json_path);
                        log::info!("Migrated settings from JSON to YAML format");
                    }
                    return settings;
                }
            }
        }

        Self::default()
    }

    /// Save settings to disk (YAML format)
    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or("Could not determine config directory")?;

        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let contents =
            serde_yml::to_string(self).map_err(|e| format!("Failed to serialize: {}", e))?;

        std::fs::write(&path, contents).map_err(|e| format!("Failed to write settings: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn dock_icon_is_hidden_by_default() {
        assert!(!Settings::default().show_in_dock);
    }

    #[test]
    fn settings_without_dock_preference_keep_icon_hidden() {
        let settings: Settings = serde_yml::from_str("{}").expect("settings should deserialize");

        assert!(!settings.show_in_dock);
    }
}
