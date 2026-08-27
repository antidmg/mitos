use tui::{
    buffer::Buffer,
    style::Style as TuiStyle,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget},
};
use view::{
    graphics::{Rect, Style},
    theme::Color,
};

pub fn render(
    surface: &mut Buffer,
    area: Rect,
    viewport_height: u16,
    content_height: usize,
    position: usize,
    bordered: bool,
    style: Style,
) {
    let thumb = style.fg.unwrap_or(Color::Reset);
    let track = style.bg.unwrap_or(Color::Reset);
    let symbol = if bordered { "▌" } else { "▐" };
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_symbol(symbol)
        .thumb_style(TuiStyle::default().fg(thumb.into()))
        .track_symbol((!bordered).then_some(symbol))
        .track_style(TuiStyle::default().fg(track.into()));
    let mut state = ScrollbarState::new(content_height)
        .position(position)
        .viewport_content_length(viewport_height as usize);
    scrollbar.render(area, surface, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bordered_scrollbar_preserves_its_track() {
        let area = Rect::new(0, 0, 5, 4);
        let mut surface = Buffer::with_lines(["....|", "....|", "....|", "....|"]);

        render(
            &mut surface,
            area,
            area.height,
            8,
            0,
            true,
            Style::default(),
        );

        let edge: Vec<_> = (0..area.height)
            .map(|y| surface[(area.right() - 1, y)].symbol())
            .collect();
        assert!(edge.contains(&"▌"));
        assert!(edge.contains(&"|"));
    }

    #[test]
    fn borderless_scrollbar_draws_its_track() {
        let area = Rect::new(0, 0, 5, 4);
        let mut surface = Buffer::empty(area);

        render(
            &mut surface,
            area,
            area.height,
            8,
            0,
            false,
            Style::default(),
        );

        assert!((0..area.height).all(|y| surface[(area.right() - 1, y)].symbol() == "▐"));
    }
}
