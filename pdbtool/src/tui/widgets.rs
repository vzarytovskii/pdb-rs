use ratatui::layout::Constraint;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Row, Table, TableState};

#[derive(Clone, Debug)]
pub enum WidgetType<'a> {
    Paragraph(Paragraph<'a>),
    Table(Table<'a>),
    List(List<'a>),
    HexDump(Paragraph<'a>),
}
impl<'a> WidgetType<'a> {
    pub fn with_focus_style(self, is_focused: bool) -> Self {
        let style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };

        match self {
            WidgetType::Paragraph(p) => {
                WidgetType::Paragraph(p.block(Block::bordered().border_style(style)))
            }
            WidgetType::Table(t) => {
                WidgetType::Table(t.block(Block::bordered().border_style(style)))
            }
            WidgetType::List(l) => WidgetType::List(l.block(Block::bordered().border_style(style))),
            WidgetType::HexDump(h) => {
                WidgetType::HexDump(h.block(Block::bordered().border_style(style)))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct WidgetWithConstraint<'a> {
    pub widget: WidgetType<'a>,
    pub constraint: Constraint,
}

impl<'a> WidgetWithConstraint<'a> {
    pub fn new(widget: WidgetType<'a>, constraint: Constraint) -> Self {
        Self { widget, constraint }
    }
}

/// Container for multiple widgets with layout constraints
#[derive(Clone, Debug)]
pub struct MultiWidget<'a> {
    pub widgets: Vec<WidgetWithConstraint<'a>>,
}

impl<'a> MultiWidget<'a> {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
        }
    }

    pub fn single(widget: WidgetType<'a>, constraint: Constraint) -> Self {
        Self {
            widgets: vec![WidgetWithConstraint::new(widget, constraint)],
        }
    }

    pub fn add(mut self, widget: WidgetType<'a>, constraint: Constraint) -> Self {
        self.widgets
            .push(WidgetWithConstraint::new(widget, constraint));
        self
    }

    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    pub fn constraints(&self) -> Vec<Constraint> {
        self.widgets.iter().map(|w| w.constraint).collect()
    }
}

impl<'a> Default for MultiWidget<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget state management for different widget types
#[derive(Clone, Debug)]
#[derive(Default)]
pub enum WidgetState {
    #[default]
    None,
    Table(TableState),
    List(ListState),
}


impl WidgetState {
    pub fn as_table_mut(&mut self) -> Option<&mut TableState> {
        match self {
            WidgetState::Table(state) => Some(state),
            _ => None,
        }
    }

    pub fn as_list_mut(&mut self) -> Option<&mut ListState> {
        match self {
            WidgetState::List(state) => Some(state),
            _ => None,
        }
    }
}

/// Helper functions for creating common widgets
pub mod helpers {
    use ratatui::widgets::{block::Position, HighlightSpacing, Padding};

    use super::*;

    pub fn create_paragraph(content: String) -> WidgetType<'static> {
        WidgetType::Paragraph(Paragraph::new(content))
    }

    pub fn create_table(rows: Vec<Row<'static>>) -> WidgetType<'static> {
        WidgetType::Table(
            Table::default()
                .rows(rows)
                .highlight_symbol("│ ")
                .highlight_spacing(HighlightSpacing::Always)
                .row_highlight_style(Style::default().fg(Color::Cyan))
                .highlight_spacing(HighlightSpacing::Always)
        )
    }

    pub fn create_list(items: Vec<ListItem<'static>>) -> WidgetType<'static> {
        let item_count = items.len();
        let title = if item_count > 1 {
            format!("Items ({} total) - ↑/↓: navigate, PgUp/PgDn: page", item_count)
        } else {
            "Items".to_string()
        };

        WidgetType::List(
            List::new(items)
                .highlight_style(Style::default().fg(Color::Cyan))
                .highlight_symbol("│ ")
                .highlight_spacing(HighlightSpacing::Always)
                .repeat_highlight_symbol(true)
                .scroll_padding(0)
                .block(Block::bordered().padding(Padding::new(1, 0, 1, 0)).title(title).title_position(Position::Bottom)),
        )
    }

    pub fn create_error_widget(error: &str) -> WidgetType<'static> {
        create_paragraph(format!("Error: {error}"))
    }

    /// Create a hex dump widget from raw bytes
    pub fn create_hex_dump(data: &[u8], start_offset: usize, max_lines: Option<usize>) -> WidgetType<'static> {
        create_hex_dump_with_width(data, start_offset, max_lines, None)
    }

    /// Create a hex dump widget with dynamic width calculation
    pub fn create_hex_dump_with_width(data: &[u8], start_offset: usize, max_lines: Option<usize>, available_width: Option<u16>) -> WidgetType<'static> {
        let mut content = String::new();
        
        // Calculate optimal bytes per line based on available width
        // Format: "xxxxxxxx: xx xx xx xx xx xx xx xx  xx xx xx xx xx xx xx xx │aaaaaaaaaaaaaaaa│"
        // Base width: 8 (offset) + 2 (": ") + 3 (space and separators) + 2 (│...│) = 15
        // Each byte needs: 3 chars for hex + 1 char for ASCII = 4 chars per byte
        // Extra space in middle adds 1 char every 8 bytes
        let bytes_per_line = if let Some(width) = available_width {
            let usable_width = width.saturating_sub(20); // Account for borders and padding
            let max_bytes = (usable_width as usize).saturating_sub(15) / 4; // 15 is base, 4 chars per byte
            std::cmp::max(8, std::cmp::min(32, max_bytes)) // Clamp between 8 and 32 bytes
        } else {
            24 // Increased default from 16 to 24 for better utilization
        };
        
        let limit = max_lines.unwrap_or(50); // Default to 50 lines
        
        let mut lines_written = 0;
        for (i, chunk) in data.chunks(bytes_per_line).enumerate() {
            if lines_written >= limit {
                content.push_str(&format!("\n... ({} more bytes not shown)", data.len() - i * bytes_per_line));
                break;
            }
            
            let offset = start_offset + i * bytes_per_line;
            
            // Offset column
            content.push_str(&format!("{:08x}: ", offset));
            
            // Hex bytes
            for (j, byte) in chunk.iter().enumerate() {
                if j == bytes_per_line / 2 {
                    content.push(' '); // Extra space in the middle
                }
                content.push_str(&format!("{:02x} ", byte));
            }
            
            // Pad if we have fewer bytes than bytes_per_line
            if chunk.len() < bytes_per_line {
                for j in chunk.len()..bytes_per_line {
                    if j == bytes_per_line / 2 {
                        content.push(' ');
                    }
                    content.push_str("   ");
                }
            }
            
            // ASCII representation
            content.push_str(" │");
            for byte in chunk {
                let c = if byte.is_ascii_graphic() || *byte == b' ' {
                    *byte as char
                } else {
                    '.'
                };
                content.push(c);
            }
            content.push('│');
            
            if i < data.chunks(bytes_per_line).len() - 1 || lines_written < limit - 1 {
                content.push('\n');
            }
            
            lines_written += 1;
        }
        
        let title = format!("Hex Dump (showing {} bytes from offset 0x{:x})", 
                          std::cmp::min(data.len(), limit * bytes_per_line), start_offset);
        
        WidgetType::HexDump(
            Paragraph::new(content)
                .block(Block::bordered()
                    .padding(Padding::new(1, 1, 1, 1))
                    .title(title)
                    .title_position(Position::Top))
        )
    }
}
