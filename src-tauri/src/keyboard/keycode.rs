use tauri_plugin_global_shortcut::Code;

/// Keys that can be used in the Edit Popup shortcut or typed by ovim itself.
///
/// Each entry maps a key to its macOS virtual key code
/// (https://developer.apple.com/documentation/carbon/1430449-virtual_key_codes),
/// the name stored in settings, a display name, and its W3C key code.
macro_rules! define_keycodes {
    (
        $(
            $variant:ident = ($raw:expr, $name:expr, $display:expr, $code:ident)
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum KeyCode {
            $($variant),*
        }

        impl KeyCode {
            pub fn as_raw(self) -> u16 {
                match self {
                    $(Self::$variant => $raw,)*
                }
            }

            /// Snake_case name used in settings
            pub fn to_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name.to_lowercase().as_str() {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn to_display_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $display,)*
                }
            }

            pub fn to_code(self) -> Code {
                match self {
                    $(Self::$variant => Code::$code,)*
                }
            }

            pub fn from_code(code: Code) -> Option<Self> {
                match code {
                    $(Code::$code => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

define_keycodes! {
    // Letters
    A = (0x00, "a", "A", KeyA),
    S = (0x01, "s", "S", KeyS),
    D = (0x02, "d", "D", KeyD),
    F = (0x03, "f", "F", KeyF),
    H = (0x04, "h", "H", KeyH),
    G = (0x05, "g", "G", KeyG),
    Z = (0x06, "z", "Z", KeyZ),
    X = (0x07, "x", "X", KeyX),
    C = (0x08, "c", "C", KeyC),
    V = (0x09, "v", "V", KeyV),
    B = (0x0B, "b", "B", KeyB),
    Q = (0x0C, "q", "Q", KeyQ),
    W = (0x0D, "w", "W", KeyW),
    E = (0x0E, "e", "E", KeyE),
    R = (0x0F, "r", "R", KeyR),
    Y = (0x10, "y", "Y", KeyY),
    T = (0x11, "t", "T", KeyT),
    O = (0x1F, "o", "O", KeyO),
    U = (0x20, "u", "U", KeyU),
    I = (0x22, "i", "I", KeyI),
    P = (0x23, "p", "P", KeyP),
    L = (0x25, "l", "L", KeyL),
    J = (0x26, "j", "J", KeyJ),
    K = (0x28, "k", "K", KeyK),
    N = (0x2D, "n", "N", KeyN),
    M = (0x2E, "m", "M", KeyM),

    // Numbers
    Num1 = (0x12, "1", "1", Digit1),
    Num2 = (0x13, "2", "2", Digit2),
    Num3 = (0x14, "3", "3", Digit3),
    Num4 = (0x15, "4", "4", Digit4),
    Num5 = (0x17, "5", "5", Digit5),
    Num6 = (0x16, "6", "6", Digit6),
    Num7 = (0x1A, "7", "7", Digit7),
    Num8 = (0x1C, "8", "8", Digit8),
    Num9 = (0x19, "9", "9", Digit9),
    Num0 = (0x1D, "0", "0", Digit0),

    // Special keys
    Return = (0x24, "return", "Return", Enter),
    Tab = (0x30, "tab", "Tab", Tab),
    Space = (0x31, "space", "Space", Space),
    Delete = (0x33, "delete", "Delete", Backspace),
    Escape = (0x35, "escape", "Escape", Escape),

    // Arrow keys
    Left = (0x7B, "left", "Left Arrow", ArrowLeft),
    Right = (0x7C, "right", "Right Arrow", ArrowRight),
    Down = (0x7D, "down", "Down Arrow", ArrowDown),
    Up = (0x7E, "up", "Up Arrow", ArrowUp),

    // Function keys
    F1 = (0x7A, "f1", "F1", F1),
    F2 = (0x78, "f2", "F2", F2),
    F3 = (0x63, "f3", "F3", F3),
    F4 = (0x76, "f4", "F4", F4),
    F5 = (0x60, "f5", "F5", F5),
    F6 = (0x61, "f6", "F6", F6),
    F7 = (0x62, "f7", "F7", F7),
    F8 = (0x64, "f8", "F8", F8),
    F9 = (0x65, "f9", "F9", F9),
    F10 = (0x6D, "f10", "F10", F10),
    F11 = (0x67, "f11", "F11", F11),
    F12 = (0x6F, "f12", "F12", F12),

    // Navigation
    Home = (0x73, "home", "Home", Home),
    End = (0x77, "end", "End", End),
    PageUp = (0x74, "page_up", "Page Up", PageUp),
    PageDown = (0x79, "page_down", "Page Down", PageDown),
    ForwardDelete = (0x75, "forward_delete", "Forward Delete", Delete),

    // Punctuation
    Equal = (0x18, "equal", "=", Equal),
    Minus = (0x1B, "minus", "-", Minus),
    LeftBracket = (0x21, "left_bracket", "[", BracketLeft),
    RightBracket = (0x1E, "right_bracket", "]", BracketRight),
    Quote = (0x27, "quote", "'", Quote),
    Semicolon = (0x29, "semicolon", ";", Semicolon),
    Backslash = (0x2A, "backslash", "\\", Backslash),
    Comma = (0x2B, "comma", ",", Comma),
    Slash = (0x2C, "slash", "/", Slash),
    Period = (0x2F, "period", ".", Period),
    Grave = (0x32, "grave", "`", Backquote),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn names_and_codes_round_trip() {
        for name in ["e", "1", "f5", "space", "left_bracket", "grave", "forward_delete"] {
            let key = KeyCode::from_name(name).expect("known key");
            assert_eq!(key.to_name(), name);
            assert_eq!(KeyCode::from_code(key.to_code()), Some(key));
        }
    }

    #[test]
    fn dom_codes_map_to_setting_names() {
        let name = |code: &str| {
            KeyCode::from_code(Code::from_str(code).expect("DOM code")).map(KeyCode::to_name)
        };
        assert_eq!(name("KeyE"), Some("e"));
        assert_eq!(name("Digit7"), Some("7"));
        assert_eq!(name("BracketLeft"), Some("left_bracket"));
        assert_eq!(name("Backspace"), Some("delete"));
        assert_eq!(name("MetaLeft"), None);
    }
}
