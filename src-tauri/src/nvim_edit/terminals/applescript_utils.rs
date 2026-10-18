//! AppleScript utilities for window management

use std::process::Command;

/// Set window size using AppleScript
pub fn set_window_size(app_name: &str, width: u32, height: u32) {
    let script = format!(
        r#"
        tell application "System Events"
            tell process "{}"
                try
                    set size of front window to {{{}, {}}}
                end try
            end tell
        end tell
        "#,
        app_name, width, height
    );

    let _ = Command::new("osascript").arg("-e").arg(&script).output();
}

/// Set window bounds by title (position and size in one call)
pub fn set_window_bounds_by_title(process_names: &[&str], title: &str, x: i32, y: i32, width: u32, height: u32) {
    let name_conditions: Vec<String> = process_names
        .iter()
        .map(|n| format!("name is \"{}\"", n))
        .collect();
    let condition = name_conditions.join(" or ");

    let script = format!(
        r#"
        tell application "System Events"
            repeat with p in (every process whose {})
                try
                    repeat with w in windows of p
                        if name of w contains "{}" then
                            set position of w to {{{}, {}}}
                            set size of w to {{{}, {}}}
                            return "ok"
                        end if
                    end repeat
                end try
            end repeat
            return "no_window_found"
        end tell
        "#,
        condition, title, x, y, width, height
    );

    log::debug!(
        "Setting window '{}' to {}x{} at ({}, {})",
        title, width, height, x, y
    );

    let output = Command::new("osascript").arg("-e").arg(&script).output();

    if let Ok(out) = output {
        if !out.status.success() {
            log::error!(
                "AppleScript set bounds failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

/// Focus a window by process names and title (without bringing all app windows to front)
pub fn focus_window_by_title(process_names: &[&str], title: &str) {
    let name_conditions: Vec<String> = process_names
        .iter()
        .map(|n| format!("name is \"{}\"", n))
        .collect();
    let condition = name_conditions.join(" or ");

    let script = format!(
        r#"
        tell application "System Events"
            repeat with p in (every process whose {})
                try
                    repeat with w in windows of p
                        if name of w contains "{}" then
                            -- Raise just this window to the front
                            perform action "AXRaise" of w
                            -- Set frontmost to give keyboard focus
                            set frontmost of p to true
                            return "ok"
                        end if
                    end repeat
                end try
            end repeat
        end tell
        "#,
        condition, title
    );

    log::info!("Focusing window with title '{}'", title);

    let output = Command::new("osascript").arg("-e").arg(&script).output();

    if let Ok(out) = output {
        if !out.status.success() {
            log::error!(
                "Failed to focus window: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

/// Show a hidden/off-screen window by moving it to position and raising it.
/// Used for pre-warmed terminal windows that are spawned off-screen.
pub fn show_and_position_window_by_title(
    process_names: &[&str],
    title: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) {
    let name_conditions: Vec<String> = process_names
        .iter()
        .map(|n| format!("name is \"{}\"", n))
        .collect();
    let condition = name_conditions.join(" or ");

    let script = format!(
        r#"
        tell application "System Events"
            repeat with p in (every process whose {})
                try
                    repeat with w in windows of p
                        if name of w contains "{}" then
                            set position of w to {{{}, {}}}
                            set size of w to {{{}, {}}}
                            perform action "AXRaise" of w
                            set frontmost of p to true
                            return "ok"
                        end if
                    end repeat
                end try
            end repeat
            return "no_window_found"
        end tell
        "#,
        condition, title, x, y, width, height
    );

    log::info!(
        "Showing prewarm window '{}' at ({}, {}) size {}x{}",
        title, x, y, width, height
    );

    let output = Command::new("osascript").arg("-e").arg(&script).output();

    match output {
        Ok(out) => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if result == "no_window_found" {
                log::warn!("Could not find prewarm window '{}' to reposition", title);
            } else if !out.status.success() {
                log::error!(
                    "AppleScript show_and_position failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            } else {
                log::info!("Prewarm window repositioned successfully");
            }
        }
        Err(e) => {
            log::error!("Failed to run AppleScript: {}", e);
        }
    }
}
