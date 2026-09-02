//! Table wrapper for editor-specific truncation behavior.
//!
//! Ratatui's table is still used for layout and rendering. This wrapper keeps
//! the historical Mitos API and can truncate fixed-width cells from the start,
//! preserving filenames and other useful suffixes.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Rect},
    style::Style,
    text::{Span, Text},
    widgets::{
        Cell as RatatuiCell, Row as RatatuiRow, StatefulWidget, Table as RatatuiTable, TableState,
    },
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// Styled text stored in one table cell.
pub struct Cell<'a> {
    /// Cell contents; line height is determined by Ratatui during rendering.
    pub content: Text<'a>,
}

impl<'a, T> From<T> for Cell<'a>
where
    T: Into<Text<'a>>,
{
    fn from(content: T) -> Self {
        Self {
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// A row of cells with a shared base style.
pub struct Row<'a> {
    /// Cells rendered from left to right.
    pub cells: Vec<Cell<'a>>,
    style: Style,
}

impl<'a> Row<'a> {
    /// Builds a row from values convertible to [`Cell`].
    pub fn new<T>(cells: T) -> Self
    where
        T: IntoIterator,
        T::Item: Into<Cell<'a>>,
    {
        Self {
            cells: cells.into_iter().map(Into::into).collect(),
            ..Default::default()
        }
    }

    /// Applies a base style to every cell in this row.
    pub fn style<S: Into<Style>>(mut self, style: S) -> Self {
        self.style = style.into();
        self
    }
}

impl<'a, T: Into<Cell<'a>>> From<T> for Row<'a> {
    fn from(cell: T) -> Self {
        Self::new([cell.into()])
    }
}

#[derive(Debug, Clone)]
/// Builder for a stateful Ratatui table with optional start truncation.
pub struct Table<'a> {
    rows: Vec<Row<'a>>,
    widths: Vec<Constraint>,
    style: Style,
    highlight_style: Style,
    highlight_symbol: Option<&'a str>,
    column_spacing: u16,
    header: Option<Row<'a>>,
}

impl<'a> Table<'a> {
    /// Builds a table from its body rows.
    pub fn new<T>(rows: T) -> Self
    where
        T: IntoIterator<Item = Row<'a>>,
    {
        Self {
            rows: rows.into_iter().collect(),
            widths: Vec::new(),
            style: Style::default(),
            highlight_style: Style::default(),
            highlight_symbol: None,
            column_spacing: 1,
            header: None,
        }
    }

    /// Sets the Ratatui constraints used to lay out columns.
    pub fn widths(mut self, widths: &[Constraint]) -> Self {
        self.widths = widths.to_vec();
        self
    }

    /// Applies the table's base style.
    pub fn style<S: Into<Style>>(mut self, style: S) -> Self {
        self.style = style.into();
        self
    }

    /// Sets the style for the row selected in [`TableState`].
    pub fn highlight_style<S: Into<Style>>(mut self, style: S) -> Self {
        self.highlight_style = style.into();
        self
    }

    /// Sets the marker rendered beside the selected row.
    pub fn highlight_symbol(mut self, symbol: &'a str) -> Self {
        self.highlight_symbol = Some(symbol);
        self
    }

    /// Sets the number of cells between columns.
    pub fn column_spacing(mut self, spacing: u16) -> Self {
        self.column_spacing = spacing;
        self
    }

    /// Adds a non-selectable header row.
    pub fn header(mut self, header: Row<'a>) -> Self {
        self.header = Some(header);
        self
    }

    /// Renders the table and updates `state` through Ratatui.
    ///
    /// When `truncate_start` is true, cells whose columns use
    /// [`Constraint::Length`] keep their suffix and receive a leading ellipsis.
    /// Other constraint kinds are left to Ratatui unchanged because their final
    /// width is not known here.
    pub fn render_table(
        self,
        area: Rect,
        buffer: &mut Buffer,
        state: &mut TableState,
        truncate_start: bool,
    ) {
        let widths = self.widths.clone();
        let rows = self
            .rows
            .into_iter()
            .map(|row| row.into_ratatui(&widths, truncate_start));
        let mut table = RatatuiTable::new(rows, self.widths)
            .style(self.style)
            .row_highlight_style(self.highlight_style)
            .column_spacing(self.column_spacing)
            .flex(Flex::Legacy);
        if let Some(symbol) = self.highlight_symbol {
            table = table.highlight_symbol(symbol);
        }
        if let Some(header) = self.header {
            table = table.header(header.into_ratatui(&widths, false));
        }

        StatefulWidget::render(table, area, buffer, state);
    }
}

impl<'a> Row<'a> {
    fn into_ratatui(self, widths: &[Constraint], truncate_start: bool) -> RatatuiRow<'a> {
        let cells = self.cells.into_iter().enumerate().map(|(index, mut cell)| {
            if truncate_start && let Some(Constraint::Length(width)) = widths.get(index) {
                truncate_text_start(&mut cell.content, *width as usize);
            }
            RatatuiCell::new(cell.content)
        });
        RatatuiRow::new(cells).style(self.style)
    }
}

fn truncate_text_start(text: &mut Text<'_>, width: usize) {
    for line in &mut text.lines {
        if line.width() <= width || width == 0 {
            continue;
        }

        let mut remaining = width.saturating_sub(1);
        let mut spans = Vec::new();
        for span in line.spans.iter().rev() {
            let mut content = String::new();
            for grapheme in span.content.graphemes(true).rev() {
                let grapheme_width = grapheme.width();
                if grapheme_width > remaining {
                    break;
                }
                content.insert_str(0, grapheme);
                remaining -= grapheme_width;
            }
            if !content.is_empty() {
                spans.push(Span::styled(content, span.style));
            }
            if remaining == 0 {
                break;
            }
        }
        spans.reverse();
        spans.insert(0, Span::raw("…"));
        line.spans = spans;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_ratatui_and_preserves_start_truncation() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        let mut state = TableState::default();
        let table = Table::new([Row::new(["abcdef"])]).widths(&[Constraint::Length(5)]);

        table.render_table(buffer.area, &mut buffer, &mut state, true);

        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert_eq!(rendered, "…cdef");
    }

    #[test]
    fn last_column_fills_remaining_width() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
        let mut state = TableState::default();
        let table = Table::new([Row::new(["a", "bcde"])])
            .widths(&[Constraint::Length(1), Constraint::Length(1)])
            .column_spacing(0);

        table.render_table(buffer.area, &mut buffer, &mut state, false);

        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert_eq!(rendered, "abcde");
    }
}
