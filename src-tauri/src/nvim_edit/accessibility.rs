//! Accessibility APIs for getting text from focused UI elements

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::CFString;

#[allow(non_upper_case_globals)]
const kAXValueCGPointType: i32 = 1;
#[allow(non_upper_case_globals)]
const kAXValueCGSizeType: i32 = 2;
#[allow(non_upper_case_globals)]
const kAXValueCFRangeType: i32 = 4;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: CFTypeRef,
        attribute: CFTypeRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(
        value: CFTypeRef,
        the_type: i32,
        value_ptr: *mut std::ffi::c_void,
    ) -> bool;
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
}

/// Wrapper for AXUIElementRef that can be sent across threads
/// The element is retained on creation and released on drop
pub struct AXElementHandle {
    element: CFTypeRef,
}

impl AXElementHandle {
    /// Create a new handle by retaining the element
    pub unsafe fn new(element: CFTypeRef) -> Option<Self> {
        if element.is_null() {
            return None;
        }
        CFRetain(element);
        Some(Self { element })
    }

    /// Get the raw element pointer
    pub fn as_ptr(&self) -> CFTypeRef {
        self.element
    }
}

impl Drop for AXElementHandle {
    fn drop(&mut self) {
        if !self.element.is_null() {
            unsafe { CFRelease(self.element) };
        }
    }
}

// Safety: AXUIElementRef is safe to send across threads
unsafe impl Send for AXElementHandle {}
unsafe impl Sync for AXElementHandle {}

impl std::fmt::Debug for AXElementHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AXElementHandle")
            .field("element", &self.element)
            .finish()
    }
}

impl Clone for AXElementHandle {
    fn clone(&self) -> Self {
        unsafe {
            if !self.element.is_null() {
                CFRetain(self.element);
            }
            Self { element: self.element }
        }
    }
}

/// RAII wrapper for CFTypeRef that automatically releases the reference when dropped.
struct CFHandle(CFTypeRef);

impl CFHandle {
    /// Create a new handle from a CFTypeRef. Returns None if the pointer is null.
    fn new(ptr: CFTypeRef) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self(ptr))
        }
    }

    /// Get an attribute value from this element
    fn get_attribute(&self, attr_name: &str) -> Option<CFHandle> {
        let attr = CFString::new(attr_name);
        let mut value: CFTypeRef = std::ptr::null();
        let result =
            unsafe { AXUIElementCopyAttributeValue(self.0, attr.as_CFTypeRef(), &mut value) };
        if result != 0 || value.is_null() {
            None
        } else {
            Some(CFHandle(value))
        }
    }

    /// Extract a CGPoint from an AXValue
    fn extract_point(&self) -> Option<core_graphics::geometry::CGPoint> {
        let mut point = core_graphics::geometry::CGPoint::new(0.0, 0.0);
        let extracted = unsafe {
            AXValueGetValue(
                self.0,
                kAXValueCGPointType,
                &mut point as *mut _ as *mut std::ffi::c_void,
            )
        };
        if extracted {
            Some(point)
        } else {
            None
        }
    }

    /// Extract a CGSize from an AXValue
    fn extract_size(&self) -> Option<core_graphics::geometry::CGSize> {
        let mut size = core_graphics::geometry::CGSize::new(0.0, 0.0);
        let extracted = unsafe {
            AXValueGetValue(
                self.0,
                kAXValueCGSizeType,
                &mut size as *mut _ as *mut std::ffi::c_void,
            )
        };
        if extracted {
            Some(size)
        } else {
            None
        }
    }

    /// Extract a CFRange from an AXValue.
    fn extract_range(&self) -> Option<(isize, isize)> {
        let mut range = core_foundation::base::CFRange::init(0, 0);
        let extracted = unsafe {
            AXValueGetValue(
                self.0,
                kAXValueCFRangeType,
                &mut range as *mut _ as *mut std::ffi::c_void,
            )
        };
        extracted.then_some((range.location, range.length))
    }

    /// Extract a signed integer from a CFNumber.
    fn extract_i64(&self) -> Option<i64> {
        let mut value = 0_i64;
        let extracted = unsafe {
            core_foundation::number::CFNumberGetValue(
                self.0 as core_foundation::number::CFNumberRef,
                core_foundation::number::kCFNumberSInt64Type,
                &mut value as *mut i64 as *mut std::ffi::c_void,
            )
        };
        extracted.then_some(value)
    }

    /// Convert to CFString and get as Rust String.
    /// Note: This consumes the handle to avoid double-free.
    fn into_string(self) -> Option<String> {
        // wrap_under_create_rule takes ownership, so we need to prevent
        // our Drop from also releasing
        let cf_string: CFString = unsafe { CFString::wrap_under_create_rule(self.0 as _) };
        let result = cf_string.to_string();
        // Prevent double-free by forgetting self (CFString now owns the ref)
        std::mem::forget(self);
        Some(result)
    }

    /// Convert a CFArray of CFStrings into owned Rust strings.
    fn to_string_array(&self) -> Vec<String> {
        let type_id = unsafe { core_foundation::base::CFGetTypeID(self.0) };
        let array_type_id = unsafe { core_foundation::array::CFArrayGetTypeID() };
        if type_id != array_type_id {
            return Vec::new();
        }

        let count = unsafe { core_foundation::array::CFArrayGetCount(self.0 as _) };

        (0..count)
            .filter_map(|index| {
                let value =
                    unsafe { core_foundation::array::CFArrayGetValueAtIndex(self.0 as _, index) };
                if value.is_null() {
                    return None;
                }

                let value = unsafe { CFString::wrap_under_get_rule(value as _) };
                Some(value.to_string())
            })
            .collect()
    }
}

impl Drop for CFHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Context about the focused application for later restoration
#[derive(Debug, Clone)]
pub struct FocusContext {
    pub app_pid: i32,
    pub app_bundle_id: String,
    /// The focused UI element (if captured), refocused when the editor closes
    pub focused_element: Option<AXElementHandle>,
}

/// Capture the current focus context (which app is focused)
pub fn capture_focus_context() -> Option<FocusContext> {
    unsafe {
        use objc::{class, msg_send, sel, sel_impl};

        let workspace: *mut objc::runtime::Object =
            msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }

        let app: *mut objc::runtime::Object = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }

        let pid: i32 = msg_send![app, processIdentifier];

        let bundle_id: *mut objc::runtime::Object = msg_send![app, bundleIdentifier];
        if bundle_id.is_null() {
            return None;
        }

        let utf8: *const std::os::raw::c_char = msg_send![bundle_id, UTF8String];
        if utf8.is_null() {
            return None;
        }

        let bundle_id_str = std::ffi::CStr::from_ptr(utf8)
            .to_string_lossy()
            .into_owned();

        // Also capture the focused UI element so focus can return to it
        let focused_element = capture_focused_element();

        Some(FocusContext {
            app_pid: pid,
            app_bundle_id: bundle_id_str,
            focused_element,
        })
    }
}

/// Capture a handle to the currently focused UI element
fn capture_focused_element() -> Option<AXElementHandle> {
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return None;
        }

        // Get focused application
        let focused_app_attr = CFString::new("AXFocusedApplication");
        let mut focused_app: CFTypeRef = std::ptr::null();
        let result = AXUIElementCopyAttributeValue(
            system_wide,
            focused_app_attr.as_CFTypeRef(),
            &mut focused_app,
        );

        if result != 0 || focused_app.is_null() {
            CFRelease(system_wide);
            return None;
        }

        // Get focused UI element from the application
        let focused_element_attr = CFString::new("AXFocusedUIElement");
        let mut focused_element: CFTypeRef = std::ptr::null();
        let result = AXUIElementCopyAttributeValue(
            focused_app,
            focused_element_attr.as_CFTypeRef(),
            &mut focused_element,
        );

        CFRelease(focused_app);
        CFRelease(system_wide);

        if result != 0 || focused_element.is_null() {
            return None;
        }

        // Create a handle (which will retain the element)
        let handle = AXElementHandle::new(focused_element);

        // Release our copy since AXElementHandle retained it
        CFRelease(focused_element);

        handle
    }
}

/// Restore focus to a previously captured application and element
pub fn restore_focus(context: &FocusContext) -> Result<(), String> {
    log::info!("Attempting to restore focus to PID {}", context.app_pid);

    unsafe {
        use objc::{class, msg_send, sel, sel_impl};

        let running_app_class = class!(NSRunningApplication);
        let app: *mut objc::runtime::Object = msg_send![
            running_app_class,
            runningApplicationWithProcessIdentifier: context.app_pid
        ];

        if app.is_null() {
            log::error!(
                "Could not find running application with PID {}",
                context.app_pid
            );
            return Err(format!(
                "Could not find running application with PID {}",
                context.app_pid
            ));
        }

        // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2
        let options: u64 = 2;
        let success: bool = msg_send![app, activateWithOptions: options];

        if !success {
            log::error!("Failed to activate application");
            return Err("Failed to activate application".to_string());
        }

        log::info!("Successfully activated application");

        // Try to restore focus to the specific element if we have it
        if let Some(ref element) = context.focused_element {
            // Small delay for app activation to complete
            std::thread::sleep(std::time::Duration::from_millis(50));

            // Try to set focus on the element using AXFocused attribute
            let focused_attr = CFString::new("AXFocused");
            let cf_true = core_foundation::boolean::CFBoolean::true_value();
            let result = AXUIElementSetAttributeValue(
                element.as_ptr(),
                focused_attr.as_CFTypeRef(),
                cf_true.as_CFTypeRef(),
            );

            if result == 0 {
                log::info!("Successfully set focus on element");
            } else {
                log::info!("Could not set focus on element (error {}), app is active though", result);
            }
        }

        Ok(())
    }
}

/// Position and size of a UI element
#[derive(Debug, Clone)]
pub struct ElementFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Get the frame of the frontmost window of the focused application
pub fn get_focused_window_frame() -> Option<ElementFrame> {
    let context = capture_focus_context()?;
    get_window_frame_for_pid(context.app_pid)
}

/// Get the frame of the focused window for a specific application PID
pub fn get_window_frame_for_pid(pid: i32) -> Option<ElementFrame> {
    let app_element = CFHandle::new(unsafe { AXUIElementCreateApplication(pid) })?;

    let focused_window = app_element.get_attribute("AXFocusedWindow").or_else(|| {
        log::warn!(
            "get_window_frame_for_pid: Failed to get AXFocusedWindow for pid {}",
            pid
        );
        None
    })?;

    let position_value = focused_window.get_attribute("AXPosition").or_else(|| {
        log::warn!("get_window_frame_for_pid: Failed to get AXPosition");
        None
    })?;

    let size_value = focused_window.get_attribute("AXSize").or_else(|| {
        log::warn!("get_window_frame_for_pid: Failed to get AXSize");
        None
    })?;

    let point = position_value.extract_point().or_else(|| {
        log::warn!("get_window_frame_for_pid: Failed to extract CGPoint from AXPosition");
        None
    })?;

    let size = size_value.extract_size().or_else(|| {
        log::warn!("get_window_frame_for_pid: Failed to extract CGSize from AXSize");
        None
    })?;

    log::info!(
        "get_window_frame_for_pid: Got frame x={}, y={}, w={}, h={}",
        point.x,
        point.y,
        size.width,
        size.height
    );

    Some(ElementFrame {
        x: point.x,
        y: point.y,
        width: size.width,
        height: size.height,
    })
}

/// Get the position and size of the currently focused UI element
pub fn get_focused_element_frame() -> Option<ElementFrame> {
    let system_wide = CFHandle::new(unsafe { AXUIElementCreateSystemWide() })?;
    let focused_app = system_wide.get_attribute("AXFocusedApplication")?;
    let focused_element = focused_app.get_attribute("AXFocusedUIElement")?;

    let position_value = focused_element.get_attribute("AXPosition")?;
    let size_value = focused_element.get_attribute("AXSize")?;

    let point = position_value.extract_point()?;
    let size = size_value.extract_size()?;

    Some(ElementFrame {
        x: point.x,
        y: point.y,
        width: size.width,
        height: size.height,
    })
}

/// Get the full text value from the currently focused UI element
pub fn get_focused_element_text(app_bundle_id: &str) -> Option<String> {
    let system_wide = CFHandle::new(unsafe { AXUIElementCreateSystemWide() })?;
    let focused_app = system_wide.get_attribute("AXFocusedApplication")?;
    let focused_element = focused_app.get_attribute("AXFocusedUIElement")?;
    let value = focused_element.get_attribute("AXValue")?;
    let text = value.into_string()?;
    let placeholder = focused_element
        .get_attribute("AXPlaceholderValue")
        .and_then(CFHandle::into_string);
    let labels = ["AXDescription", "AXTitle", "AXHelp"]
        .into_iter()
        .filter_map(|attribute| {
            focused_element
                .get_attribute(attribute)
                .and_then(CFHandle::into_string)
        })
        .collect::<Vec<_>>();
    let character_count = focused_element
        .get_attribute("AXNumberOfCharacters")
        .and_then(|value| value.extract_i64());
    let selected_range = focused_element
        .get_attribute("AXSelectedTextRange")
        .and_then(|value| value.extract_range());
    let classes = focused_element
        .get_attribute("AXDOMClassList")
        .map_or_else(Vec::new, |value| value.to_string_array());

    Some(remove_placeholder_text(
        text,
        placeholder.as_deref(),
        &labels,
        character_count,
        selected_range,
        &classes,
        app_bundle_id,
    ))
}

fn remove_placeholder_text(
    text: String,
    placeholder: Option<&str>,
    labels: &[String],
    character_count: Option<i64>,
    selected_range: Option<(isize, isize)>,
    classes: &[String],
    app_bundle_id: &str,
) -> String {
    let matches_placeholder = placeholder.is_some_and(|value| !value.is_empty() && text == value);
    let has_no_editable_characters = character_count == Some(0) && !text.is_empty();
    let matches_empty_field_label = selected_range == Some((0, 0))
        && labels
            .iter()
            .any(|label| !label.is_empty() && text == *label);
    // Signal's Quill editor exposes its generated placeholder as AXValue with
    // a trailing newline and marks the editor with its ql-blank class, but does
    // not publish AXPlaceholderValue or a label. Its selection location can be
    // either zero or the length of the visible placeholder depending on focus.
    let matches_signal_generated_placeholder = app_bundle_id == "org.whispersystems.signal-desktop"
        && classes.iter().any(|class| class == "ql-blank")
        && text
            .strip_suffix('\n')
            .is_some_and(|content| !content.is_empty() && !content.contains('\n'));

    if matches_placeholder
        || has_no_editable_characters
        || matches_empty_field_label
        || matches_signal_generated_placeholder
    {
        String::new()
    } else {
        text
    }
}

/// Get the bounds of the screen containing a given point
/// Returns the screen frame (x, y, width, height) in screen coordinates
pub fn get_screen_bounds_for_point(x: f64, y: f64) -> Option<ElementFrame> {
    unsafe {
        use objc::{class, msg_send, sel, sel_impl};

        // Get all screens
        let screens: *mut objc::runtime::Object = msg_send![class!(NSScreen), screens];
        if screens.is_null() {
            return None;
        }

        let count: usize = msg_send![screens, count];
        if count == 0 {
            return None;
        }

        // Find the screen containing the point
        for i in 0..count {
            let screen: *mut objc::runtime::Object = msg_send![screens, objectAtIndex: i];
            if screen.is_null() {
                continue;
            }

            // Get the screen's frame (in Cocoa coordinates - origin at bottom-left)
            let frame: core_graphics::geometry::CGRect = msg_send![screen, frame];

            // Convert Cocoa coordinates to screen coordinates (origin at top-left)
            // In Cocoa, y=0 is at the bottom, but we work with y=0 at top
            // The main screen's height gives us the reference
            let main_screen: *mut objc::runtime::Object = msg_send![class!(NSScreen), mainScreen];
            let main_frame: core_graphics::geometry::CGRect = msg_send![main_screen, frame];
            let main_height = main_frame.size.height;

            // Convert y coordinate: screen_y = main_height - cocoa_y - screen_height
            let screen_y = main_height - frame.origin.y - frame.size.height;

            // Check if point is within this screen (using screen coordinates)
            let screen_left = frame.origin.x;
            let screen_right = frame.origin.x + frame.size.width;
            let screen_top = screen_y;
            let screen_bottom = screen_y + frame.size.height;

            if x >= screen_left && x < screen_right && y >= screen_top && y < screen_bottom {
                return Some(ElementFrame {
                    x: screen_left,
                    y: screen_top,
                    width: frame.size.width,
                    height: frame.size.height,
                });
            }
        }

        // Fallback to main screen if point not found on any screen
        let main_screen: *mut objc::runtime::Object = msg_send![class!(NSScreen), mainScreen];
        if main_screen.is_null() {
            return None;
        }

        let frame: core_graphics::geometry::CGRect = msg_send![main_screen, frame];

        Some(ElementFrame {
            x: frame.origin.x,
            y: 0.0, // Main screen top is always 0 in screen coordinates
            width: frame.size.width,
            height: frame.size.height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::remove_placeholder_text;

    fn normalize(
        text: &str,
        placeholder: Option<&str>,
        labels: &[&str],
        character_count: Option<i64>,
        selected_range: Option<(isize, isize)>,
    ) -> String {
        normalize_for_app(
            text,
            placeholder,
            labels,
            character_count,
            selected_range,
            "",
        )
    }

    fn normalize_for_app(
        text: &str,
        placeholder: Option<&str>,
        labels: &[&str],
        character_count: Option<i64>,
        selected_range: Option<(isize, isize)>,
        app_bundle_id: &str,
    ) -> String {
        normalize_for_app_with_classes(
            text,
            placeholder,
            labels,
            character_count,
            selected_range,
            &[],
            app_bundle_id,
        )
    }

    fn normalize_for_app_with_classes(
        text: &str,
        placeholder: Option<&str>,
        labels: &[&str],
        character_count: Option<i64>,
        selected_range: Option<(isize, isize)>,
        classes: &[&str],
        app_bundle_id: &str,
    ) -> String {
        remove_placeholder_text(
            text.to_string(),
            placeholder,
            &labels
                .iter()
                .map(|label| (*label).to_string())
                .collect::<Vec<_>>(),
            character_count,
            selected_range,
            &classes
                .iter()
                .map(|class| (*class).to_string())
                .collect::<Vec<_>>(),
            app_bundle_id,
        )
    }

    #[test]
    fn matching_placeholder_is_treated_as_empty() {
        assert_eq!(normalize("Message", Some("Message"), &[], None, None), "");
    }

    #[test]
    fn real_text_is_preserved() {
        assert_eq!(
            normalize("Hello", Some("Message"), &[], None, None),
            "Hello"
        );
    }

    #[test]
    fn empty_text_is_preserved() {
        assert_eq!(
            normalize("", Some("Message"), &[], Some(0), Some((0, 0))),
            ""
        );
    }

    #[test]
    fn value_without_placeholder_is_preserved() {
        assert_eq!(normalize("Message", None, &[], None, None), "Message");
    }

    #[test]
    fn value_with_zero_editable_characters_is_treated_as_empty() {
        assert_eq!(normalize("Message", None, &[], Some(0), Some((0, 0))), "");
    }

    #[test]
    fn empty_field_accessible_label_is_treated_as_empty() {
        assert_eq!(
            normalize("Message", None, &["Message"], Some(7), Some((0, 0))),
            ""
        );
    }

    #[test]
    fn matching_label_with_active_cursor_is_preserved() {
        assert_eq!(
            normalize("Message", None, &["Message"], Some(7), Some((7, 0))),
            "Message"
        );
    }

    #[test]
    fn signal_generated_placeholder_with_trailing_newline_is_empty() {
        assert_eq!(
            normalize_for_app_with_classes(
                "Message\n",
                None,
                &[],
                Some(8),
                Some((7, 0)),
                &["ql-editor", "ql-editor--loaded", "ql-blank"],
                "org.whispersystems.signal-desktop",
            ),
            ""
        );
    }

    #[test]
    fn signal_typed_message_is_preserved() {
        assert_eq!(
            normalize_for_app_with_classes(
                "Message",
                None,
                &[],
                Some(7),
                Some((7, 0)),
                &["ql-editor", "ql-editor--loaded"],
                "org.whispersystems.signal-desktop",
            ),
            "Message"
        );
    }

    #[test]
    fn signal_text_with_trailing_newline_and_active_cursor_is_preserved() {
        assert_eq!(
            normalize_for_app_with_classes(
                "Message\n",
                None,
                &[],
                Some(8),
                Some((8, 0)),
                &["ql-editor", "ql-editor--loaded"],
                "org.whispersystems.signal-desktop",
            ),
            "Message\n"
        );
    }
}
