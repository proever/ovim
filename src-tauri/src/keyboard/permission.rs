// ApplicationServices framework bindings for macOS privacy permissions.
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef)
        -> bool;
}

/// Check whether macOS allows ovim to use the Accessibility API.
pub fn check_accessibility_permission() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}
