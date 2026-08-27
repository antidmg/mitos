use crate::compositor::{Component, Context};
use tui::buffer::Buffer as Surface;
use tui::text::Text;
use tui::widgets::{Block, Paragraph, Widget};
use view::graphics::{Margin, Rect};
use view::info::Info;

impl Component for Info {
    fn render(&mut self, viewport: Rect, surface: &mut Surface, cx: &mut Context) {
        let text_style = cx.editor.theme.get("ui.text.info");
        let popup_style = cx.editor.theme.get("ui.popup.info");

        let width = self.width + 2 + 2; // +2 for border, +2 for margin
        let height = self.height + 2; // +2 for border
        let area = viewport.intersection(Rect::new(
            viewport.right().saturating_sub(width),
            viewport.bottom().saturating_sub(height),
            width,
            height,
        ));
        surface.clear_with(area, popup_style);

        let block = Block::bordered()
            .title(self.title.as_ref())
            .border_style(popup_style);

        let margin = Margin::new(1, 0);
        let inner = block.inner(area).inner(margin);
        block.render(area, surface);

        Paragraph::new(Text::from(self.text.as_str()))
            .style(text_style)
            .render(inner, surface);
    }
}
use tui::buffer::BufferExt as _;
