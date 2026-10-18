use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use super::keycode::KeyCode;

/// Modifier keys held during a synthesized key press
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

impl Modifiers {
    const SHIFT_MASK: u64 = 0x00020000;
    const CONTROL_MASK: u64 = 0x00040000;
    const OPTION_MASK: u64 = 0x00080000;
    const COMMAND_MASK: u64 = 0x00100000;

    fn to_cg_flags(self) -> u64 {
        let mut flags = 0u64;
        if self.shift {
            flags |= Self::SHIFT_MASK;
        }
        if self.control {
            flags |= Self::CONTROL_MASK;
        }
        if self.option {
            flags |= Self::OPTION_MASK;
        }
        if self.command {
            flags |= Self::COMMAND_MASK;
        }
        flags
    }
}

/// Inject a single key event
fn inject_key(keycode: KeyCode, key_down: bool, modifiers: Modifiers) -> Result<(), String> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create event source")?;

    let event = CGEvent::new_keyboard_event(source, keycode.as_raw(), key_down)
        .map_err(|_| "Failed to create keyboard event")?;

    let flags = CGEventFlags::from_bits_truncate(modifiers.to_cg_flags());
    event.set_flags(flags);

    event.post(CGEventTapLocation::HID);

    Ok(())
}

/// Inject a key press (down + up)
pub fn inject_key_press(keycode: KeyCode, modifiers: Modifiers) -> Result<(), String> {
    inject_key(keycode, true, modifiers)?;
    inject_key(keycode, false, modifiers)?;
    Ok(())
}
