use std::io;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ms_pdb::Pdb;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::{CrosstermBackend, Frame};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, List, Paragraph};
use ratatui::Terminal;

use super::content;
use super::navigation::TreeNavigator;
use super::widgets::{MultiWidget, WidgetState, WidgetType};

#[derive(Clone, Debug, PartialEq)]
pub enum FocusedPanel {
    TreeView,
    DetailsView { widget_index: usize },
    SearchInput,
}

pub struct TuiApp {
    pub pdb: Box<Pdb>,
    should_quit: bool,
    error_message: Option<String>,
    navigator: TreeNavigator,
    focused_panel: FocusedPanel,
    widget_states: Vec<WidgetState>,
    last_details_area_height: u16,
    last_details_area_width: u16,
}

impl TuiApp {
    pub fn new(pdb: Box<Pdb>, pdb_file: &Path) -> Result<Self> {
        let mut navigator = TreeNavigator::new(pdb_file);
        
        // Update navigation with actual stream information from PDB
        navigator.update_streams_from_pdb(&pdb);
        
        // Update navigation with actual symbol information from PDB
        navigator.update_symbols_from_pdb(&pdb);

        Ok(Self {
            pdb,
            should_quit: false,
            error_message: None,
            navigator,
            focused_panel: FocusedPanel::TreeView,
            widget_states: Vec::new(),
            last_details_area_height: 0,
            last_details_area_width: 0,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_app(&mut terminal);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    fn run_app(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|f| self.draw(f))?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match self.handle_key_event(key.code) {
                        Ok(()) => {}
                        Err(e) => {
                            self.error_message = Some(format!("Error: {e}"));
                        }
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let main_chunks = if self.navigator.search_mode {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(frame.area());

            // Draw search input at bottom
            self.draw_search_input(frame, chunks[1]);

            chunks[0]
        } else {
            frame.area()
        };

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(main_chunks);

        // Draw panels using a simpler approach
        self.draw_tree_panel(frame, chunks[0]);
        self.draw_details_panel(frame, chunks[1]);
    }

    fn draw_tree_panel(&mut self, frame: &mut Frame, area: ratatui::prelude::Rect) {
        let tree_style = if self.is_tree_focused() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };

        let mut title = "PDB Structure".to_string();
        if self.navigator.search_mode {
            title = format!("{title} (Search Mode)");
        }

        let tree_block = Block::bordered().title(title).border_style(tree_style);

        // Use clone to avoid borrow checker issues
        let list_items = self.navigator.get_visible_items();
        let list = List::new(list_items)
            .block(tree_block)
            .highlight_style(Style::default().fg(Color::Yellow));

        frame.render_stateful_widget(list, area, &mut self.navigator.list_state);
    }

    fn draw_details_panel(&mut self, frame: &mut Frame, area: ratatui::prelude::Rect) {
        // Store the area dimensions for pagination and layout calculations
        self.last_details_area_height = area.height;
        self.last_details_area_width = area.width;

        let content = self.get_details_content();
        self.ensure_widget_states_match_content(&content);
        self.render_multi_widget(frame, area, content);
    }

    fn draw_search_input(&self, frame: &mut Frame, area: ratatui::prelude::Rect) {
        let search_style = if matches!(self.focused_panel, FocusedPanel::SearchInput) {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };

        let search_text = if self.navigator.search_query.is_empty() {
            "Type to search...".to_string()
        } else {
            self.navigator.search_query.clone()
        };

        let search_block = Block::bordered()
            .title(format!(
                "Search ({} matches)",
                self.navigator.filtered_indices.len()
            ))
            .border_style(search_style);

        let search_paragraph = Paragraph::new(search_text).block(search_block);
        frame.render_widget(search_paragraph, area);
    }

    fn get_details_content(&self) -> MultiWidget<'static> {
        if let Some(node_id) = self.navigator.get_selected_node_id() {
            match self.create_content_for_node(&node_id) {
                Ok(content) => content,
                Err(e) => {
                    use ratatui::layout::Constraint;
                    MultiWidget::single(
                        super::widgets::helpers::create_error_widget(&format!(
                            "Error loading content: {e}"
                        )),
                        Constraint::Min(1),
                    )
                }
            }
        } else {
            let help_text = if self.navigator.search_mode {
                "Search Mode - Controls:\n• Type: Add to search query\n• Backspace: Remove from query\n• Enter: Select result\n• Esc: Exit search\n• ↑/↓: Navigate results".to_string()
            } else {
                "Select an item to view details\n\nControls:\n• Tab: Cycle focus between panels\n• ↑/↓: Navigate tree/details\n• PgUp/PgDn: Page up/down in details\n• Enter: Toggle expand/collapse nodes\n• →: Expand nodes\n• ←: Collapse or navigate to parent\n• Home/End: Jump to first/last item\n• /: Start search\n• h/l: Collapse/expand all\n• r: Refresh\n• q/Esc: Quit".to_string()
            };

            MultiWidget::single(
                WidgetType::Paragraph(Paragraph::new(help_text)),
                Constraint::Min(1),
            )
        }
    }

    fn create_content_for_node(
        &self,
        node_id: &super::navigation::TreeNodeId,
    ) -> Result<MultiWidget<'static>> {
        let handler = content::get_content_handler(node_id);
        handler.get_content_with_area(&self.pdb, self.last_details_area_width, self.last_details_area_height)
    }

    fn render_multi_widget(
        &mut self,
        frame: &mut Frame,
        area: ratatui::prelude::Rect,
        content: MultiWidget<'_>,
    ) {
        let focused_widget_index = match &self.focused_panel {
            FocusedPanel::DetailsView { widget_index } => Some(*widget_index),
            _ => None,
        };

        if content.is_empty() {
            let empty_paragraph = Paragraph::new("No content").block(Block::bordered());
            frame.render_widget(empty_paragraph, area);
            return;
        }

        if content.len() == 1 {
            let widget_with_constraint = &content.widgets[0];
            let is_focused = focused_widget_index == Some(0);
            let styled_widget = widget_with_constraint
                .widget
                .clone()
                .with_focus_style(is_focused);
            self.render_single_widget(frame, area, styled_widget, is_focused, 0);
        } else {
            let constraints = content.constraints();
            let widget_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(area);

            for (i, widget_with_constraint) in content.widgets.iter().enumerate() {
                let is_focused = focused_widget_index == Some(i);
                let styled_widget = widget_with_constraint
                    .widget
                    .clone()
                    .with_focus_style(is_focused);
                self.render_single_widget(frame, widget_chunks[i], styled_widget, is_focused, i);
            }
        }
    }

    fn render_single_widget(
        &mut self,
        frame: &mut Frame,
        area: ratatui::prelude::Rect,
        widget: WidgetType<'_>,
        _is_focused: bool,
        widget_index: usize,
    ) {
        match widget {
            WidgetType::Paragraph(paragraph) => {
                frame.render_widget(paragraph, area);
            }
            WidgetType::Table(table) => {
                if let Some(state) = self
                    .widget_states
                    .get_mut(widget_index)
                    .and_then(|ws| ws.as_table_mut())
                {
                    frame.render_stateful_widget(table, area, state);
                } else {
                    frame.render_widget(table, area);
                }
            }
            WidgetType::List(list) => {
                if let Some(state) = self
                    .widget_states
                    .get_mut(widget_index)
                    .and_then(|ws| ws.as_list_mut())
                {
                    frame.render_stateful_widget(list, area, state);
                } else {
                    frame.render_widget(list, area);
                }
            }
            WidgetType::HexDump(hex_dump) => {
                frame.render_widget(hex_dump, area);
            }
        }
    }

    fn handle_key_event(&mut self, key: KeyCode) -> Result<()> {
        // Global quit keys
        match key {
            KeyCode::Char('q') | KeyCode::Esc if !self.navigator.search_mode => {
                self.should_quit = true;
                return Ok(());
            }
            _ => {}
        }

        // Handle search mode differently
        if self.navigator.search_mode {
            return self.handle_search_key_event(key);
        }

        match key {
            KeyCode::Tab => {
                self.switch_focus();
            }
            KeyCode::Char('/') => {
                self.navigator.enter_search_mode();
                self.focused_panel = FocusedPanel::SearchInput;
            }
            KeyCode::Char('r') => {
                self.navigator.update_flat_items();
            }
            KeyCode::Char('h') => {
                self.navigator.collapse_all();
            }
            KeyCode::Char('l') => {
                self.navigator.expand_all();
            }
            // Tree navigation when tree is focused
            KeyCode::Up if self.is_tree_focused() => {
                self.navigator.move_up();
                self.reset_widget_focus();
            }
            KeyCode::Down if self.is_tree_focused() => {
                self.navigator.move_down();
                self.reset_widget_focus();
            }
            KeyCode::Enter if self.is_tree_focused() => {
                self.navigator.toggle_selected_node();
            }
            KeyCode::Right if self.is_tree_focused() => {
                self.navigator.expand_selected_node();
            }
            KeyCode::Left if self.is_tree_focused() => {
                self.navigator.collapse_or_navigate_to_parent();
            }
            // Details panel navigation when details is focused
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
                if self.is_details_focused() =>
            {
                self.handle_details_navigation(key);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_search_key_event(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => {
                self.navigator.exit_search_mode();
                self.focused_panel = FocusedPanel::TreeView;
            }
            KeyCode::Enter => {
                // Exit search and focus on selected item
                self.navigator.exit_search_mode();
                self.focused_panel = FocusedPanel::TreeView;
            }
            KeyCode::Up => {
                self.navigator.move_up();
            }
            KeyCode::Down => {
                self.navigator.move_down();
            }
            KeyCode::Char(c) => {
                let mut query = self.navigator.search_query.clone();
                query.push(c);
                self.navigator.update_search_query(query);
            }
            KeyCode::Backspace => {
                let mut query = self.navigator.search_query.clone();
                query.pop();
                self.navigator.update_search_query(query);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_details_navigation(&mut self, key: KeyCode) {
        if let FocusedPanel::DetailsView { widget_index } = &self.focused_panel {
            if let Some(widget_state) = self.widget_states.get_mut(*widget_index) {
                match key {
                    KeyCode::Up => {
                        if let Some(state) = widget_state.as_table_mut() {
                            let selected = state.selected().unwrap_or(0);
                            if selected > 0 {
                                state.select(Some(selected - 1));
                            }
                        } else if let Some(state) = widget_state.as_list_mut() {
                            state.select_previous();
                        }
                    }
                    KeyCode::Down => {
                        if let Some(state) = widget_state.as_table_mut() {
                            let selected = state.selected().unwrap_or(0);
                            state.select(Some(selected + 1));
                        } else if let Some(state) = widget_state.as_list_mut() {
                            state.select_next();
                        }
                    }
                    KeyCode::PageUp => {
                        if let Some(state) = widget_state.as_table_mut() {
                            let selected = state.selected().unwrap_or(0);
                            // For tables, use a reasonable page size based on visible area
                            let page_size = (self.last_details_area_height.saturating_sub(4)).max(1) as usize;
                            let new_selected = selected.saturating_sub(page_size);
                            state.select(Some(new_selected));
                        } else if let Some(state) = widget_state.as_list_mut() {
                            // Use the proper scroll method for lists which handles multi-line items correctly
                            let page_size = (self.last_details_area_height.saturating_sub(4)).max(1);
                            state.scroll_up_by(page_size);
                        }
                    }
                    KeyCode::PageDown => {
                        if let Some(state) = widget_state.as_table_mut() {
                            let selected = state.selected().unwrap_or(0);
                            // For tables, use a reasonable page size based on visible area
                            let page_size = (self.last_details_area_height.saturating_sub(4)).max(1) as usize;
                            state.select(Some(selected + page_size));
                        } else if let Some(state) = widget_state.as_list_mut() {
                            // Use the proper scroll method for lists which handles multi-line items correctly
                            let page_size = (self.last_details_area_height.saturating_sub(4)).max(1);
                            state.scroll_down_by(page_size);
                        }
                    }
                    KeyCode::Home => {
                        if let Some(state) = widget_state.as_table_mut() {
                            state.select(Some(0));
                        } else if let Some(state) = widget_state.as_list_mut() {
                            state.select_first();
                        }
                    }
                    KeyCode::End => {
                        if let Some(state) = widget_state.as_table_mut() {
                            state.select(Some(usize::MAX));
                        } else if let Some(state) = widget_state.as_list_mut() {
                            state.select_last();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn switch_focus(&mut self) {
        let content = match self.navigator.get_selected_node_id() {
            Some(node_id) => {
                match self.create_content_for_node(&node_id) {
                    Ok(content) => content,
                    Err(_) => {
                        // Error content - create a single error widget
                        use ratatui::layout::Constraint;
                        MultiWidget::single(
                            super::widgets::helpers::create_error_widget("Error loading content"),
                            Constraint::Min(1),
                        )
                    }
                }
            }
            None => {
                use ratatui::layout::Constraint;
                let help_text = if self.navigator.search_mode {
                    "Search Mode - Controls:\n• Type: Add to search query\n• Backspace: Remove from query\n• Enter: Select result\n• Esc: Exit search\n• ↑/↓: Navigate results"
                } else {
                    "Select an item to view details\n\nControls:\n• Tab: Cycle focus between panels\n• ↑/↓: Navigate tree\n• Enter: Toggle expand/collapse nodes\n• →: Expand nodes\n• ←: Collapse or navigate to parent\n• /: Start search\n• h/l: Collapse/expand all\n• r: Refresh\n• q/Esc: Quit"
                };
                MultiWidget::single(
                    WidgetType::Paragraph(Paragraph::new(help_text.to_string())),
                    Constraint::Min(1),
                )
            }
        };

        let current_widget_count = content.len();
        self.ensure_widget_states_match_content(&content);

        self.focused_panel = match &self.focused_panel {
            FocusedPanel::TreeView => {
                if current_widget_count > 0 {
                    FocusedPanel::DetailsView { widget_index: 0 }
                } else {
                    FocusedPanel::TreeView
                }
            }
            FocusedPanel::DetailsView { widget_index } => {
                let next_index = widget_index + 1;
                if next_index < current_widget_count {
                    FocusedPanel::DetailsView {
                        widget_index: next_index,
                    }
                } else {
                    FocusedPanel::TreeView
                }
            }
            FocusedPanel::SearchInput => FocusedPanel::TreeView,
        };
    }

    fn ensure_widget_states_match_content(&mut self, content: &MultiWidget<'_>) {
        let widget_count = content.len();
        self.widget_states.resize(widget_count, WidgetState::None);

        for (i, widget_with_constraint) in content.widgets.iter().enumerate() {
            if let Some(current_state) = self.widget_states.get(i) {
                let needs_update = match (&widget_with_constraint.widget, current_state) {
                    (WidgetType::Table(_), WidgetState::Table(_)) => false,
                    (WidgetType::List(_), WidgetState::List(_)) => false,
                    (WidgetType::Paragraph(_), WidgetState::None) => false,
                    (WidgetType::HexDump(_), WidgetState::None) => false,
                    _ => true,
                };

                if needs_update {
                    let expected_state = match &widget_with_constraint.widget {
                        WidgetType::Table(_) => WidgetState::Table(ratatui::widgets::TableState::default()),
                        WidgetType::List(_) => WidgetState::List(ratatui::widgets::ListState::default()),
                        WidgetType::Paragraph(_) => WidgetState::None,
                        WidgetType::HexDump(_) => WidgetState::None,
                    };
                    self.widget_states[i] = expected_state;
                }
            } else {
                break;
            }
        }
    }

    fn reset_widget_focus(&mut self) {
        self.focused_panel = FocusedPanel::TreeView;
        // Don't clear widget states - preserve scroll positions
    }

    pub fn is_tree_focused(&self) -> bool {
        matches!(self.focused_panel, FocusedPanel::TreeView)
    }

    pub fn is_details_focused(&self) -> bool {
        matches!(self.focused_panel, FocusedPanel::DetailsView { .. })
    }
}
