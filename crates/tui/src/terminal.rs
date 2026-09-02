//! Mitos-specific terminal session configuration.

use ui_core::terminal::KittyKeyboardProtocolConfig;

/// Terminal capabilities and modes requested for one application session.
#[derive(Debug)]
pub struct Config {
    /// Whether claiming the terminal should enable mouse event reporting.
    pub enable_mouse_capture: bool,
    /// Emit extended underline escape sequences even when capability detection
    /// does not advertise them.
    pub force_enable_extended_underlines: bool,
    /// Policy for negotiating the Kitty keyboard protocol.
    pub kitty_keyboard_protocol: KittyKeyboardProtocolConfig,
}
