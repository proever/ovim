//! Text capture from focused elements

use super::accessibility::{self, ElementFrame};
use super::browser_scripting::{self, BrowserType, CursorPosition};
use super::clipboard::capture_text_via_clipboard;

/// Result of text capture with additional context
pub struct CaptureResult {
    pub text: String,
    pub element_frame: Option<ElementFrame>,
    /// Initial cursor position (0-based line and column) if available
    pub cursor_position: Option<CursorPosition>,
    /// Browser type if this is a browser
    pub browser_target: Option<BrowserType>,
}

/// Capture text and element frame from the focused element
/// If clipboard_mode is true, always use clipboard-based capture (Cmd+A, Cmd+C)
pub fn capture_text_and_frame(
    app_bundle_id: &str,
    initial_element_frame: Option<ElementFrame>,
    clipboard_mode: bool,
) -> CaptureResult {
    let browser_target = browser_scripting::detect_browser_type(app_bundle_id);

    // If clipboard_mode is enabled, skip smart detection and use clipboard directly
    if clipboard_mode {
        log::info!("Clipboard mode enabled, using Cmd+A/Cmd+C for text capture");
        let text = capture_text_via_clipboard().unwrap_or_default();
        log::info!("Clipboard capture: {} chars", text.len());

        return CaptureResult {
            text,
            element_frame: initial_element_frame,
            cursor_position: None, // No cursor tracking in clipboard mode
            browser_target: None,  // Disable browser-specific features
        };
    }

    // For browsers, try to get text AND cursor in one JS call
    // This is more reliable as cursor position won't be affected by text capture
    if let Some(target) = browser_target {
        log::info!(
            "Text capture: attempting JS capture for browser {:?}",
            target
        );
        if let Some(result) = browser_scripting::get_browser_text_and_cursor(target) {
            if result.found {
                log::info!(
                    "Text capture: JS succeeded, {} chars, cursor={:?}",
                    result.text.len(),
                    result.cursor
                );

                // Get element frame if needed
                let element_frame = if initial_element_frame.is_none() {
                    log::info!("Getting element frame via browser scripting");
                    browser_scripting::get_browser_element_frame(target)
                } else {
                    initial_element_frame
                };

                return CaptureResult {
                    text: result.text,
                    element_frame,
                    cursor_position: result.cursor,
                    browser_target,
                };
            }
            log::info!("Text capture: JS found no editable element, falling back to clipboard");
        } else {
            log::info!("Text capture: JS failed, falling back to clipboard");
        }
    }

    // JS capture failed - fall back to accessibility/clipboard capture.
    // The browser target is dropped: cursor sync and the hostname filetype key
    // rely on the JavaScript calls that just failed.
    log::info!("JS capture failed, disabling browser scripting for this session");

    // Get element frame from browser scripting (this can still work for positioning)
    let element_frame = if initial_element_frame.is_none() {
        if let Some(target) = browser_target {
            log::info!("Getting element frame via browser scripting");
            let browser_frame = browser_scripting::get_browser_element_frame(target);
            log::info!(
                "Browser scripting element frame: {:?}",
                browser_frame
                    .as_ref()
                    .map(|f| (f.x, f.y, f.width, f.height))
            );
            browser_frame
        } else {
            None
        }
    } else {
        initial_element_frame
    };

    CaptureResult {
        text: capture_text_content(app_bundle_id),
        element_frame,
        cursor_position: None,
        browser_target: None,
    }
}

/// Capture text content from the focused element
fn capture_text_content(app_bundle_id: &str) -> String {
    // An empty AXValue is a valid value for an empty text field. Only fall back
    // to clipboard capture when the accessibility API cannot return a value.
    match accessibility::get_focused_element_text(app_bundle_id) {
        Some(text) => {
            let preview: String = text.lines().take(5).collect::<Vec<_>>().join("\\n");
            log::info!("Got text: {} chars, preview: {}", text.len(), preview);
            text
        }
        None => {
            log::info!("Accessibility text capture unavailable, trying clipboard-based capture");
            if let Some(captured) = capture_text_via_clipboard() {
                log::info!("Captured {} chars via clipboard", captured.len());
                return captured;
            }

            String::new()
        }
    }
}
