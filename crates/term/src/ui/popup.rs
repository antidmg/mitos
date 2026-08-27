use crate::{
    commands::Open,
    compositor::{Callback, Component, Context, Event, EventResult},
    ctrl, key,
};
use tui::buffer::BufferExt as _;
use tui::{
    buffer::Buffer as Surface,
    style::Style as TuiStyle,
    widgets::{Block, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};

use editor_core::Position;
use view::{
    graphics::{Margin, Rect, Style},
    input::{MouseEvent, MouseEventKind},
    Editor,
};

const MIN_HEIGHT: u16 = 6;
const MAX_HEIGHT: u16 = 26;
const MAX_WIDTH: u16 = 120;

struct RenderInfo {
    area: Rect,
    child_height: u16,
    render_borders: bool,
}

#[derive(Debug, Clone, Copy, Default)]
enum PopupKind {
    #[default]
    Popup,
    Menu,
}

fn render_scrollbar(
    surface: &mut Surface,
    area: Rect,
    viewport_height: u16,
    content_height: u16,
    scroll: usize,
    bordered: bool,
    style: Style,
) {
    let thumb = style.fg.unwrap_or(view::theme::Color::Reset);
    let track = style.bg.unwrap_or(view::theme::Color::Reset);
    let symbol = if bordered { "▌" } else { "▐" };
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_symbol(symbol)
        .thumb_style(TuiStyle::default().fg(thumb.into()))
        .track_symbol((!bordered).then_some(symbol))
        .track_style(TuiStyle::default().fg(track.into()));
    let mut state = ScrollbarState::new(content_height as usize)
        .position(scroll)
        .viewport_content_length(viewport_height as usize);
    scrollbar.render(area, surface, &mut state);
}

// TODO: share logic with Menu, it's essentially Popup(render_fn), but render fn needs to return
// a width/height hint. maybe Popup(Box<Component>)

pub struct Popup<T: Component> {
    contents: T,
    position: Option<Position>,
    area: Rect,
    position_bias: Open,
    scroll_half_pages: usize,
    auto_close: bool,
    ignore_escape_key: bool,
    id: &'static str,
    has_scrollbar: bool,
    kind: PopupKind,
}

impl<T: Component> Popup<T> {
    pub fn new(id: &'static str, contents: T) -> Self {
        Self {
            contents,
            position: None,
            position_bias: Open::Below,
            area: Rect::new(0, 0, 0, 0),
            scroll_half_pages: 0,
            auto_close: false,
            ignore_escape_key: false,
            id,
            has_scrollbar: true,
            kind: PopupKind::default(),
        }
    }

    /// Set the anchor position next to which the popup should be drawn.
    ///
    /// Note that this is not the position of the top-left corner of the rendered popup itself,
    /// but rather the screen-space position of the information to which the popup refers.
    pub fn position(mut self, pos: Option<Position>) -> Self {
        self.position = pos;
        self
    }

    pub fn get_position(&self) -> Option<Position> {
        self.position
    }

    /// Set the popup to prefer to render above or below the anchor position.
    ///
    /// This preference will be ignored if the viewport doesn't have enough space in the
    /// chosen direction.
    pub fn position_bias(mut self, bias: Open) -> Self {
        self.position_bias = bias;
        self
    }

    pub fn auto_close(mut self, auto_close: bool) -> Self {
        self.auto_close = auto_close;
        self
    }

    /// Ignores an escape keypress event, letting the outer layer
    /// (usually the editor) handle it. This is useful for popups
    /// in insert mode like completion and signature help where
    /// the popup is closed on the mode change from insert to normal
    /// which is done with the escape key. Otherwise the popup consumes
    /// the escape key event and closes it, and an additional escape
    /// would be required to exit insert mode.
    pub fn ignore_escape_key(mut self, ignore: bool) -> Self {
        self.ignore_escape_key = ignore;
        self
    }

    pub fn scroll_half_page_down(&mut self) {
        self.scroll_half_pages += 1;
    }

    pub fn scroll_half_page_up(&mut self) {
        self.scroll_half_pages = self.scroll_half_pages.saturating_sub(1);
    }

    /// Toggles the Popup's scrollbar.
    /// Consider disabling the scrollbar in case the child
    /// already has its own.
    pub fn with_scrollbar(mut self, enable_scrollbar: bool) -> Self {
        self.has_scrollbar = enable_scrollbar;
        self
    }

    pub fn menu_style(mut self) -> Self {
        self.kind = PopupKind::Menu;
        self
    }

    pub fn contents(&self) -> &T {
        &self.contents
    }

    pub fn contents_mut(&mut self) -> &mut T {
        &mut self.contents
    }

    pub fn area(&mut self, viewport: Rect, editor: &Editor) -> Rect {
        self.render_info(viewport, editor).area
    }

    fn render_info(&mut self, viewport: Rect, editor: &Editor) -> RenderInfo {
        let mut position = editor.cursor().0.unwrap_or_default();
        if let Some(old_position) = self
            .position
            .filter(|old_position| old_position.row == position.row)
        {
            position = old_position;
        } else {
            self.position = Some(position);
        }

        let mut render_borders = if matches!(self.kind, PopupKind::Menu) {
            editor.menu_border()
        } else {
            editor.popup_border()
        };

        // -- make sure frame doesn't stick out of bounds
        let mut rel_x = position.col as u16;
        let mut rel_y = position.row as u16;

        // if there's a orientation preference, use that
        // if we're on the top part of the screen, do below
        // if we're on the bottom part, do above
        let can_put_below = viewport.height > rel_y + MIN_HEIGHT;
        let can_put_above = rel_y.checked_sub(MIN_HEIGHT).is_some();
        let final_pos = match self.position_bias {
            Open::Below => match can_put_below {
                true => Open::Below,
                false => Open::Above,
            },
            Open::Above => match can_put_above {
                true => Open::Above,
                false => Open::Below,
            },
        };

        // compute maximum space available for child
        let mut max_height = match final_pos {
            Open::Above => rel_y,
            Open::Below => viewport.height.saturating_sub(1 + rel_y),
        };
        max_height = max_height.min(MAX_HEIGHT);
        let mut max_width = viewport.width.saturating_sub(2).min(MAX_WIDTH);
        render_borders = render_borders && max_height > 3 && max_width > 3;
        if render_borders {
            max_width -= 2;
            max_height -= 2;
        }

        // compute required child size and reclamp
        let (mut width, child_height) = self
            .contents
            .required_size((max_width, max_height))
            .expect("Component needs required_size implemented in order to be embedded in a popup");

        width = width.min(MAX_WIDTH);
        let height = if render_borders {
            (child_height + 2).min(MAX_HEIGHT)
        } else {
            child_height.min(MAX_HEIGHT)
        };
        if render_borders {
            width += 2;
        }
        if viewport.width <= rel_x + width + 2 {
            rel_x = viewport.width.saturating_sub(width + 2);
            width = viewport.width.saturating_sub(rel_x + 2)
        }

        let area = match final_pos {
            Open::Above => {
                rel_y = rel_y.saturating_sub(height);
                Rect::new(rel_x, rel_y, width, position.row as u16 - rel_y)
            }
            Open::Below => {
                rel_y += 1;
                let y_max = viewport.bottom().min(height + rel_y);
                Rect::new(rel_x, rel_y, width, y_max - rel_y)
            }
        };
        RenderInfo {
            area,
            child_height,
            render_borders,
        }
    }

    fn handle_mouse_event(
        &mut self,
        &MouseEvent {
            kind,
            column: x,
            row: y,
            ..
        }: &MouseEvent,
    ) -> EventResult {
        if self.auto_close && matches!(kind, MouseEventKind::Down(_)) {
            let close_fn: Callback = Box::new(|compositor, _| {
                // remove the layer
                compositor.remove(self.id.as_ref());
            });

            return EventResult::Ignored(Some(close_fn));
        }

        let mouse_is_within_popup = x >= self.area.left()
            && x < self.area.right()
            && y >= self.area.top()
            && y < self.area.bottom();

        if !mouse_is_within_popup {
            return EventResult::Ignored(None);
        }

        match kind {
            MouseEventKind::ScrollDown if self.has_scrollbar => {
                self.scroll_half_page_down();
                EventResult::Consumed(None)
            }
            MouseEventKind::ScrollUp if self.has_scrollbar => {
                self.scroll_half_page_up();
                EventResult::Consumed(None)
            }
            _ => EventResult::Ignored(None),
        }
    }
}

impl<T: Component> Component for Popup<T> {
    fn handle_event(&mut self, event: &Event, cx: &mut Context) -> EventResult {
        let key = match event {
            Event::Key(event) => *event,
            Event::Mouse(event) => return self.handle_mouse_event(event),
            Event::Resize(_, _) => {
                // TODO: calculate inner area, call component's handle_event with that area
                return EventResult::Ignored(None);
            }
            _ => return EventResult::Ignored(None),
        };

        if key!(Esc) == key && self.ignore_escape_key {
            return EventResult::Ignored(None);
        }

        let close_fn: Callback = Box::new(|compositor, _| {
            // remove the layer
            compositor.remove(self.id.as_ref());
        });

        // Code completion handles arrows and page up/down itself,
        // but code lens does not. First check whether content knows
        // about the key event. When not, check the default keys.
        match self.contents.handle_event(event, cx) {
            EventResult::Ignored(fn_once) => {
                match key {
                    // esc or ctrl-c aborts the completion and closes the menu
                    key!(Esc) | ctrl!('c') => {
                        let _ = self.contents.handle_event(event, cx);
                        EventResult::Consumed(Some(close_fn))
                    }
                    key!(PageDown) | ctrl!('d') => {
                        self.scroll_half_page_down();
                        EventResult::Consumed(None)
                    }
                    key!(PageUp) | ctrl!('u') => {
                        self.scroll_half_page_up();
                        EventResult::Consumed(None)
                    }
                    _ => {
                        // for some events, we want to process them but send ignore, specifically all input except
                        // tab/enter/ctrl-k or whatever will confirm the selection/ ctrl-n/ctrl-p for scroll.

                        if self.auto_close {
                            EventResult::Ignored(Some(close_fn))
                        } else {
                            EventResult::Ignored(fn_once)
                        }
                    }
                }
            }
            ev => ev,
        }
    }

    fn render(&mut self, viewport: Rect, surface: &mut Surface, cx: &mut Context) {
        let RenderInfo {
            area,
            child_height,
            render_borders,
        } = self.render_info(viewport, cx.editor);
        self.area = area;

        // clear area
        let background = if matches!(self.kind, PopupKind::Menu) {
            // TODO: consistently style menu
            cx.editor
                .theme
                .try_get("ui.menu")
                .unwrap_or_else(|| cx.editor.theme.get("ui.text"))
        } else {
            cx.editor.theme.get("ui.popup")
        };
        surface.clear_with(area, background);

        let mut inner = area;
        if render_borders {
            inner = area.inner(Margin::new(1, 1));
            Widget::render(Block::bordered(), area, surface);
        }
        let max_offset = child_height.saturating_sub(inner.height) as usize;
        let half_page_size = (inner.height / 2) as usize;
        let scroll = max_offset.min(self.scroll_half_pages * half_page_size);
        self.scroll_half_pages = scroll
            .checked_div(half_page_size)
            .unwrap_or(self.scroll_half_pages);
        cx.scroll = Some(scroll);
        self.contents.render(inner, surface, cx);

        if self.has_scrollbar && child_height > inner.height {
            let scroll_style = cx.editor.theme.get("ui.menu.scroll");
            render_scrollbar(
                surface,
                area,
                inner.height,
                child_height,
                scroll,
                render_borders,
                scroll_style,
            );
        }
    }

    fn id(&self) -> Option<&'static str> {
        Some(self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_preserves_a_border_track() {
        let area = Rect::new(0, 0, 5, 4);
        let mut surface = Surface::with_lines(["....|", "....|", "....|", "....|"]);

        render_scrollbar(
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
        let mut surface = Surface::empty(area);

        render_scrollbar(
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
