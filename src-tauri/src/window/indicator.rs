use tauri::WebviewWindow;

/// Set up the indicator window with special properties
pub fn setup_indicator_window(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    #[allow(deprecated)] // cocoa crate is deprecated, but objc2-app-kit migration is future work
    {
        use cocoa::appkit::NSWindowCollectionBehavior;
        use cocoa::base::id;

        let ns_window = window.ns_window().map_err(|e| e.to_string())? as id;

        unsafe {
            use objc::*;
            let _: () = msg_send![ns_window, setLevel: 3i64]; // NSFloatingWindowLevel

            use cocoa::appkit::NSWindow;
            ns_window.setCollectionBehavior_(
                NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                    | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary,
            );
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = window;

    Ok(())
}
