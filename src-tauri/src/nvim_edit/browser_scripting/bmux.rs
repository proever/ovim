use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct Response<T> {
    ok: bool,
    result: Option<T>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    focused_client_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Client {
    id: String,
    pane_id: Option<String>,
}

static BMUX_CLI: LazyLock<Option<PathBuf>> = LazyLock::new(find_cli);

fn find_cli() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("BMUX_BIN") {
        candidates.push(PathBuf::from(path));
    }
    candidates.push(PathBuf::from(
        "/Applications/bmux.app/Contents/Resources/bin/bmux",
    ));
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/bmux.app/Contents/Resources/bin/bmux"));
        candidates.push(home.join("workspace/_tools/bmux.app/Contents/Resources/bin/bmux"));
    }

    candidates.into_iter().find(|path| path.is_file())
}

fn run<T: for<'de> Deserialize<'de>>(args: &[&str]) -> Result<T, String> {
    let executable = BMUX_CLI
        .as_ref()
        .ok_or_else(|| "bmux CLI not found".to_string())?;
    // Finder launches have a minimal PATH; the bmux launcher needs Node.
    let node = crate::nvim_edit::terminals::process_utils::resolve_command_path("node");
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if let Some(directory) = std::path::Path::new(&node)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        paths.insert(0, directory.to_path_buf());
    }
    let path = std::env::join_paths(paths)
        .map_err(|error| format!("Invalid bmux command PATH: {error}"))?;
    let output = Command::new(executable)
        .env("PATH", path)
        .args(args)
        .output()
        .map_err(|error| format!("Failed to run bmux CLI: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "bmux CLI failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let response: Response<T> = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid bmux response: {error}"))?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "bmux command failed".to_string()));
    }
    response
        .result
        .ok_or_else(|| "bmux response had no result".to_string())
}

pub fn focused_pane_id() -> Result<String, String> {
    let diagnostics: Diagnostics = run(&["diagnostics"])?;
    let focused_client_id = diagnostics
        .focused_client_id
        .ok_or_else(|| "bmux has no focused client".to_string())?;
    let clients: Vec<Client> = run(&["list-clients"])?;
    clients
        .into_iter()
        .find(|client| client.id == focused_client_id)
        .and_then(|client| client.pane_id)
        .ok_or_else(|| "Focused bmux client has no selected pane".to_string())
}

pub fn execute_javascript(pane_id: &str, javascript: &str) -> Result<String, String> {
    run(&["eval", "-t", pane_id, javascript])
}

pub fn replace_text(
    pane_id: &str,
    text: &str,
    target_element_id: Option<&str>,
) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct PreparedInput {
        kind: String,
        token: String,
        append: bool,
        #[serde(rename = "nativeInput")]
        native_input: bool,
        text: String,
    }
    let javascript = super::javascript::build_prepare_bmux_input_js(target_element_id, text);
    let prepared = execute_javascript(pane_id, &javascript)?;
    let prepared: PreparedInput = serde_json::from_str(&prepared)
        .map_err(|_| "bmux editor preparation failed".to_string())?;
    if !matches!(prepared.kind.as_str(), "draftjs" | "editable") {
        return Err(format!("Unsupported bmux editor kind: {}", prepared.kind));
    }

    let replacement = (|| -> Result<(), String> {
        if prepared.append && prepared.text.is_empty() {
            return Ok(());
        }
        // Draft.js tracks selection in its editor state. A DOM-only collapse
        // can leave that state at the old caret, so move it with a trusted key.
        // Preparation already sets native input selection; those editors do
        // not have a separate selection model to synchronize with key events.
        if !prepared.native_input && prepared.append {
            // CDP bypasses AppKit's Command+Down translation on macOS.
            let _: serde_json::Value = run(&[
                "cdp", "-t", pane_id, "Input.dispatchKeyEvent",
                r#"{"type":"keyDown","key":"End","code":"End","windowsVirtualKeyCode":35,"commands":["moveToEndOfDocument"]}"#,
            ])?;
            let _: serde_json::Value = run(&[
                "cdp", "-t", pane_id, "Input.dispatchKeyEvent",
                r#"{"type":"keyUp","key":"End","code":"End","windowsVirtualKeyCode":35}"#,
            ])?;
        } else if !prepared.native_input {
            let _: serde_json::Value = run(&["key", "-t", pane_id, "Meta+A"])?;
        }
        if prepared.text.is_empty() {
            let _: serde_json::Value = run(&["key", "-t", pane_id, "Backspace"])?;
        } else {
            // Preparation already focused the deep editor. A document-level
            // selector cannot resolve editors inside shadow DOM (Reddit chat).
            // Draft.js needs trusted Enter events to update its block model;
            // inserting a multiline string can leave only the last line.
            let lines: Vec<&str> = if prepared.kind == "draftjs" {
                prepared.text.split('\n').collect()
            } else {
                // In chat textareas Enter can send a message. Insert the
                // whole string through the trusted text channel instead.
                vec![prepared.text.as_str()]
            };
            for (index, line) in lines.into_iter().enumerate() {
                if index > 0 {
                    let _: serde_json::Value = run(&["key", "-t", pane_id, "Enter"])?;
                }
                if !line.is_empty() {
                    let _: serde_json::Value = run(&["type", "-t", pane_id, "--text", line])?;
                }
            }
        }
        Ok(())
    })();
    // Always restore normal selection styling, including failed replacements.
    let cleanup = format!(
        "(function() {{ var cleanups = window.__ovimInputCleanups; var cleanup = cleanups && cleanups[{}]; if (cleanup) cleanup(); return 'ok'; }})()",
        serde_json::to_string(&prepared.token)
            .map_err(|error| error.to_string())?,
    );
    if let Err(error) = execute_javascript(pane_id, &cleanup) {
        log::warn!("Could not restore editor selection styling: {error}");
    }
    replacement?;
    Ok(Some(prepared.token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_success_response() {
        let response: Response<String> =
            serde_json::from_str(r#"{"ok":true,"result":"done"}"#).expect("valid response");
        assert!(response.ok);
        assert_eq!(response.result.as_deref(), Some("done"));
    }
}
