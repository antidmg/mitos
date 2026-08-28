//! Mitos-specific terminal session configuration.

use ui_core::terminal::KittyKeyboardProtocolConfig;

/// Terminal configuration
#[derive(Debug)]
pub struct Config {
    pub enable_mouse_capture: bool,
    pub force_enable_extended_underlines: bool,
    pub kitty_keyboard_protocol: KittyKeyboardProtocolConfig,
}
