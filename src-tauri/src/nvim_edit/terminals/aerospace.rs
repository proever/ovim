//! Optional AeroSpace integration for Ovim's own Alacritty popup windows.

use serde::Deserialize;
use std::process::Command;
use std::time::Duration;

#[derive(Deserialize)]
struct Window {
    #[serde(rename = "window-id")]
    id: u32,
    #[serde(rename = "window-title")]
    title: String,
}

fn window_id(json: &[u8], title: &str) -> Option<u32> {
    serde_json::from_slice::<Vec<Window>>(json)
        .ok()?
        .into_iter()
        .find(|window| window.title == title)
        .map(|window| window.id)
}

/// Float only the exact Ovim popup, never the currently focused terminal.
/// Returns false when AeroSpace is absent or stopped.
pub fn float_window(title: &str) -> bool {
    if !title.starts_with("ovim-edit-") && !title.starts_with("ovim-prewarm-") {
        return false;
    }
    let executable = super::process_utils::resolve_command_path("aerospace");
    for attempt in 0..10 {
        let output = match Command::new(&executable)
            .args([
                "list-windows", "--monitor", "all", "--app-bundle-id", "org.alacritty",
                "--format", "%{window-id} %{window-title}", "--json",
            ])
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return false,
        };
        if let Some(id) = window_id(&output.stdout, title) {
            return match Command::new(&executable)
                .args(["layout", "--window-id", &id.to_string(), "floating"])
                .output()
            {
                Ok(output) if output.status.success() => true,
                _ => {
                    log::warn!("Could not float Ovim window in AeroSpace");
                    false
                }
            };
        }
        // AeroSpace may discover a new window after the editor socket is ready.
        if attempt < 9 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    log::warn!("AeroSpace did not discover Ovim window {title}");
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_exact_popup_not_other_terminal_or_session() {
        let windows = br#"[
            {"window-id":1,"window-title":"shell"},
            {"window-id":2,"window-title":"ovim-edit-1234"},
            {"window-id":3,"window-title":"ovim-edit-123"},
            {"window-id":4,"window-title":"ovim-prewarm-123"}
        ]"#;
        assert_eq!(window_id(windows, "ovim-edit-123"), Some(3));
        assert_eq!(window_id(windows, "ovim-prewarm-123"), Some(4));
        assert_eq!(window_id(windows, "ovim-edit-12"), None);
        assert_eq!(window_id(b"invalid", "ovim-edit-123"), None);
    }
}
