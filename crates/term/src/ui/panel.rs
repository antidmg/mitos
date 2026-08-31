use tui::widgets::Block;
use view::Theme;

pub use tui::panel::{
    content_area, top_border, BORDER_INSET, HORIZONTAL_INSET, PADDED_HORIZONTAL_INSET,
    PADDED_VERTICAL_INSET, VERTICAL_INSET,
};

pub fn bordered(theme: &Theme) -> Block<'static> {
    tui::panel::bordered(theme.get("ui.window"))
}

pub fn horizontally_padded(theme: &Theme) -> Block<'static> {
    tui::panel::horizontally_padded(theme.get("ui.window"))
}

pub fn uniformly_padded(theme: &Theme) -> Block<'static> {
    tui::panel::uniformly_padded(theme.get("ui.window"))
}
