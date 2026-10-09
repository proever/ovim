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
    let javascript = super::javascript::build_prepare_bmux_input_js(target_element_id);
    let prepared = execute_javascript(pane_id, &javascript)?;
    let (kind, token) = prepared
        .strip_prefix("ok_")
        .and_then(|value| value.split_once(':'))
        .ok_or_else(|| format!("bmux editor preparation failed: {prepared}"))?;
    let selector = match kind {
        "draftjs" => format!(
            "[data-ovim-editor=\"{token}\"] [contenteditable=\"true\"]"
        ),
        "editable" => format!("[data-ovim-editor=\"{token}\"]"),
        _ => return Err(format!("Unsupported bmux editor kind: {kind}")),
    };

    let replacement = (|| -> Result<(), String> {
        let _: serde_json::Value = run(&["key", "-t", pane_id, "Meta+A"])?;
        let _: serde_json::Value = if text.is_empty() {
            run(&["key", "-t", pane_id, "Backspace"])?
        } else {
            run(&["type", "-t", pane_id, "--selector", &selector, "--text", text])?
        };
        Ok(())
    })();
    // Always restore normal selection styling, including failed replacements.
    let cleanup = format!(
        "(function() {{ var el = document.querySelector({}); if (el && el.__ovimInputCleanup) el.__ovimInputCleanup(); return 'ok'; }})()",
        serde_json::to_string(&format!("[data-ovim-editor=\"{token}\"]"))
            .map_err(|error| error.to_string())?,
    );
    if let Err(error) = execute_javascript(pane_id, &cleanup) {
        log::warn!("Could not restore editor selection styling: {error}");
    }
    replacement?;
    Ok(Some(token.to_string()))
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
