use tui::{
    layout::Rect,
    widgets::{Block, Borders, Padding},
};
use view::{graphics::Style, Theme};

pub const PADDING: u16 = 1;
pub const HORIZONTAL_INSET: u16 = PADDING * 2;
pub const VERTICAL_INSET: u16 = PADDING * 2;

pub fn bordered(theme: &Theme) -> Block<'static> {
    Block::bordered().border_style(theme.get("ui.window"))
}

pub fn horizontally_padded(theme: &Theme) -> Block<'static> {
    bordered(theme).padding(Padding::horizontal(PADDING))
}

pub fn top_border(style: Style) -> Block<'static> {
    Block::new().borders(Borders::TOP).border_style(style)
}

pub fn content_area(area: Rect) -> Rect {
    Block::new().padding(Padding::uniform(PADDING)).inner(area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_area_applies_a_uniform_inset() {
        assert_eq!(Rect::new(4, 6, 8, 3), content_area(Rect::new(3, 5, 10, 5)));
    }

    #[test]
    fn content_area_saturates_for_tiny_panels() {
        assert_eq!(Rect::new(4, 6, 0, 0), content_area(Rect::new(3, 5, 1, 1)));
    }
}
