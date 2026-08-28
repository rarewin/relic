use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::path::Path;

use crate::buffer::Buffer;
use crate::completion::{complete_path, expand_tilde};
use crate::syntax::{SyntaxRegistry, highlight_line};

#[derive(Debug, PartialEq, Eq)]
pub enum PrefixKey {
    None,
    CtrlX,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptType {
    SaveAs,
    ConfirmOverwrite(String),
}

#[derive(Debug, Clone)]
pub struct PromptState {
    pub prompt_type: PromptType,
    pub label: String,
    pub input: String,
    pub cursor: usize,
    pub completions_hint: Option<String>,
}

pub struct Editor {
    pub buffer: Buffer,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub prefix: PrefixKey,
    pub prompt: Option<PromptState>,
    pub message: Option<String>,
    pub should_quit: bool,
    pub syntax_registry: SyntaxRegistry,
}

impl Editor {
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            prefix: PrefixKey::None,
            prompt: None,
            message: Some(
                "Welcome to Relic! (C-x C-s: Save, C-x C-w: Write file, C-x C-c: Quit)".into(),
            ),
            should_quit: false,
            syntax_registry: SyntaxRegistry::new(),
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // 1. If currently inside a Minibuffer prompt
        if self.prompt.is_some() {
            self.handle_prompt_key(key, is_ctrl);
            return;
        }

        // 2. Normal Editor key handling
        match self.prefix {
            PrefixKey::None => {
                if is_ctrl {
                    match key.code {
                        // Emacs basic navigation
                        KeyCode::Char('n') => self.move_down(),
                        KeyCode::Char('p') => self.move_up(),
                        KeyCode::Char('f') => self.move_right(),
                        KeyCode::Char('b') => self.move_left(),
                        KeyCode::Char('a') => self.move_to_bol(),
                        KeyCode::Char('e') => self.move_to_eol(),
                        KeyCode::Char('d') => self.delete_char(),
                        KeyCode::Char('h') => self.delete_backspace(),
                        KeyCode::Char('k') => self.kill_line(),
                        KeyCode::Char('x') => {
                            self.prefix = PrefixKey::CtrlX;
                            self.message = Some("C-x".into());
                        }
                        KeyCode::Char('g') => {
                            self.message = Some("Quit".into());
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Up => self.move_up(),
                        KeyCode::Down => self.move_down(),
                        KeyCode::Left => self.move_left(),
                        KeyCode::Right => self.move_right(),
                        KeyCode::Home => self.move_to_bol(),
                        KeyCode::End => self.move_to_eol(),
                        KeyCode::Backspace => self.delete_backspace(),
                        KeyCode::Delete => self.delete_char(),
                        KeyCode::Enter => self.insert_newline(),
                        KeyCode::Tab => {
                            for _ in 0..4 {
                                self.insert_char(' ');
                            }
                        }
                        KeyCode::Char(c) => self.insert_char(c),
                        KeyCode::Esc => {
                            self.message = None;
                        }
                        _ => {}
                    }
                }
            }
            PrefixKey::CtrlX => {
                self.prefix = PrefixKey::None;
                if is_ctrl {
                    match key.code {
                        KeyCode::Char('s') => self.request_save(),
                        KeyCode::Char('w') => self.request_write_file(),
                        KeyCode::Char('c') => self.should_quit = true,
                        KeyCode::Char('g') => {
                            self.message = Some("Quit".into());
                        }
                        _ => {
                            self.message = Some(format!(
                                "C-x C-{} is undefined",
                                match key.code {
                                    KeyCode::Char(c) => c.to_string(),
                                    _ => "?".into(),
                                }
                            ));
                        }
                    }
                } else {
                    match key.code {
                        KeyCode::Char('s') => self.request_save(),
                        KeyCode::Char('w') => self.request_write_file(),
                        KeyCode::Char('c') => self.should_quit = true,
                        KeyCode::Char('g') | KeyCode::Esc => {
                            self.message = Some("Quit".into());
                        }
                        _ => {
                            self.message = Some(format!(
                                "C-x {} is undefined",
                                match key.code {
                                    KeyCode::Char(c) => c.to_string(),
                                    _ => "?".into(),
                                }
                            ));
                        }
                    }
                }
            }
        }
    }

    fn handle_prompt_key(&mut self, key: KeyEvent, is_ctrl: bool) {
        let prompt_type = self.prompt.as_ref().map(|p| p.prompt_type.clone());

        match prompt_type {
            Some(PromptType::ConfirmOverwrite(target_path)) => {
                // Confirm Overwrite dialog: Y / N
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.prompt = None;
                        self.do_save_to(&target_path);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.prompt = None;
                        self.message = Some("Cancelled".into());
                    }
                    KeyCode::Char('g') if is_ctrl => {
                        self.prompt = None;
                        self.message = Some("Cancelled".into());
                    }
                    _ => {
                        self.message = Some("Please answer y or n.".into());
                    }
                }
            }
            Some(PromptType::SaveAs) => {
                if is_ctrl {
                    match key.code {
                        KeyCode::Char('g') => {
                            self.prompt = None;
                            self.message = Some("Quit".into());
                        }
                        KeyCode::Char('h') => {
                            self.prompt_backspace();
                        }
                        KeyCode::Char('a') => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = 0;
                                p.completions_hint = None;
                            }
                        }
                        KeyCode::Char('e') => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = p.input.chars().count();
                                p.completions_hint = None;
                            }
                        }
                        KeyCode::Char('b') => {
                            self.prompt_move_left();
                        }
                        KeyCode::Char('f') => {
                            self.prompt_move_right();
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Tab => {
                            self.prompt_complete();
                        }
                        KeyCode::Enter => {
                            self.confirm_prompt();
                        }
                        KeyCode::Esc => {
                            self.prompt = None;
                            self.message = Some("Quit".into());
                        }
                        KeyCode::Backspace => {
                            self.prompt_backspace();
                        }
                        KeyCode::Left => {
                            self.prompt_move_left();
                        }
                        KeyCode::Right => {
                            self.prompt_move_right();
                        }
                        KeyCode::Home => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = 0;
                                p.completions_hint = None;
                            }
                        }
                        KeyCode::End => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = p.input.chars().count();
                                p.completions_hint = None;
                            }
                        }
                        KeyCode::Char(c) => {
                            self.prompt_insert_char(c);
                        }
                        _ => {}
                    }
                }
            }
            None => {}
        }
    }

    fn prompt_complete(&mut self) {
        if let Some(ref mut p) = self.prompt {
            let res = complete_path(&p.input);
            if let Some(completed) = res.completed {
                p.input = completed;
                p.cursor = p.input.chars().count();
            }

            if res.candidates.is_empty() {
                p.completions_hint = Some("[No match]".into());
            } else if res.candidates.len() == 1 {
                p.completions_hint = Some("[Sole completion]".into());
            } else {
                let preview = res.candidates.join("  ");
                p.completions_hint = Some(format!("[{}]", preview));
            }
        }
    }

    fn prompt_move_left(&mut self) {
        if let Some(ref mut p) = self.prompt {
            p.cursor = p.cursor.saturating_sub(1);
            p.completions_hint = None;
        }
    }

    fn prompt_move_right(&mut self) {
        if let Some(ref mut p) = self.prompt {
            let max = p.input.chars().count();
            if p.cursor < max {
                p.cursor += 1;
            }
            p.completions_hint = None;
        }
    }

    fn prompt_insert_char(&mut self, c: char) {
        if let Some(ref mut p) = self.prompt {
            let mut chars: Vec<char> = p.input.chars().collect();
            chars.insert(p.cursor, c);
            p.input = chars.into_iter().collect();
            p.cursor += 1;
            p.completions_hint = None;
        }
    }

    fn prompt_backspace(&mut self) {
        if let Some(ref mut p) = self.prompt
            && p.cursor > 0
        {
            let mut chars: Vec<char> = p.input.chars().collect();
            chars.remove(p.cursor - 1);
            p.input = chars.into_iter().collect();
            p.cursor -= 1;
            p.completions_hint = None;
        }
    }

    fn confirm_prompt(&mut self) {
        if let Some(prompt) = self.prompt.take() {
            match prompt.prompt_type {
                PromptType::SaveAs => {
                    let trimmed = prompt.input.trim();
                    if trimmed.is_empty() {
                        self.message = Some("No file name specified".into());
                        return;
                    }

                    let expanded = expand_tilde(trimmed);
                    let target_path = Path::new(&expanded);

                    if target_path.exists() {
                        self.prompt = Some(PromptState {
                            prompt_type: PromptType::ConfirmOverwrite(trimmed.to_string()),
                            label: format!("File '{}' exists; overwrite? (y or n) ", trimmed),
                            input: String::new(),
                            cursor: 0,
                            completions_hint: None,
                        });
                    } else {
                        self.do_save_to(trimmed);
                    }
                }
                PromptType::ConfirmOverwrite(_) => {}
            }
        }
    }

    fn do_save_to(&mut self, path: &str) {
        match self.buffer.save_to(path) {
            Ok(()) => {
                self.message = Some(format!("Wrote {}", path));
            }
            Err(e) => {
                self.message = Some(format!("Error saving file: {}", e));
            }
        }
    }

    pub fn request_save(&mut self) {
        if self.buffer.path.is_some() {
            self.save_file();
        } else {
            self.prompt = Some(PromptState {
                prompt_type: PromptType::SaveAs,
                label: "Save file as: ".into(),
                input: String::new(),
                cursor: 0,
                completions_hint: None,
            });
            self.message = None;
        }
    }

    pub fn request_write_file(&mut self) {
        let default_input = self
            .buffer
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let len = default_input.chars().count();
        self.prompt = Some(PromptState {
            prompt_type: PromptType::SaveAs,
            label: "Write file: ".into(),
            input: default_input,
            cursor: len,
            completions_hint: None,
        });
        self.message = None;
    }

    pub fn save_file(&mut self) {
        match self.buffer.save() {
            Ok(()) => {
                let name = self
                    .buffer
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "[No Name]".into());
                self.message = Some(format!("Wrote {}", name));
            }
            Err(e) => {
                self.message = Some(format!("Error saving file: {}", e));
            }
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert_char(self.cursor_row, self.cursor_col, c);
        self.cursor_col += 1;
        self.message = None;
    }

    pub fn insert_newline(&mut self) {
        let (new_row, new_col) = self.buffer.insert_newline(self.cursor_row, self.cursor_col);
        self.cursor_row = new_row;
        self.cursor_col = new_col;
        self.message = None;
    }

    pub fn delete_backspace(&mut self) {
        if let Some((new_row, new_col)) = self
            .buffer
            .delete_backspace(self.cursor_row, self.cursor_col)
        {
            self.cursor_row = new_row;
            self.cursor_col = new_col;
            self.message = None;
        }
    }

    pub fn delete_char(&mut self) {
        self.buffer.delete_char(self.cursor_row, self.cursor_col);
        self.message = None;
    }

    pub fn kill_line(&mut self) {
        let current_line_len = self.buffer.line_len(self.cursor_row);
        if self.cursor_col >= current_line_len {
            self.delete_char();
        } else {
            let line_start = self.buffer.rope.line_to_char(self.cursor_row);
            let from = line_start + self.cursor_col;
            let to = line_start + current_line_len;
            if from < to {
                self.buffer.rope.remove(from..to);
                self.buffer.is_modified = true;
            }
        }
        self.message = None;
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor_col();
        }
        self.message = None;
    }

    pub fn move_down(&mut self) {
        let total_lines = self.buffer.len_lines();
        if self.cursor_row + 1 < total_lines {
            self.cursor_row += 1;
            self.clamp_cursor_col();
        }
        self.message = None;
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.buffer.line_len(self.cursor_row);
        }
        self.message = None;
    }

    pub fn move_right(&mut self) {
        let line_len = self.buffer.line_len(self.cursor_row);
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.buffer.len_lines() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
        self.message = None;
    }

    pub fn move_to_bol(&mut self) {
        self.cursor_col = 0;
        self.message = None;
    }

    pub fn move_to_eol(&mut self) {
        self.cursor_col = self.buffer.line_len(self.cursor_row);
        self.message = None;
    }

    fn clamp_cursor_col(&mut self) {
        let max_col = self.buffer.line_len(self.cursor_row);
        if self.cursor_col > max_col {
            self.cursor_col = max_col;
        }
    }

    fn update_scroll(&mut self, text_height: usize, text_width: usize) {
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if self.cursor_row >= self.scroll_row + text_height {
            self.scroll_row = self.cursor_row - text_height + 1;
        }

        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if self.cursor_col >= self.scroll_col + text_width {
            self.scroll_col = self.cursor_col - text_width + 1;
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

        let text_area = chunks[0];
        let status_area = chunks[1];
        let message_area = chunks[2];

        let total_lines = self.buffer.len_lines();
        let gutter_width = format!("{}", total_lines).len() + 2;

        let content_width = (text_area.width as usize).saturating_sub(gutter_width);
        let content_height = text_area.height as usize;

        self.update_scroll(content_height, content_width);

        let syntax_opt = self
            .buffer
            .path
            .as_ref()
            .and_then(|p| self.syntax_registry.find_syntax_for_file(p));

        // 1. Text Area Rendering
        let mut lines = Vec::new();
        for i in 0..content_height {
            let line_idx = self.scroll_row + i;
            if line_idx < total_lines {
                let gutter_str = format!("{:>width$} ", line_idx + 1, width = gutter_width - 1);
                let gutter_span = Span::styled(gutter_str, Style::default().fg(Color::DarkGray));

                let line_rope = self.buffer.rope.line(line_idx);
                let full_line_str = line_rope.to_string();
                let full_line_trimmed = full_line_str.trim_end_matches(['\r', '\n']);

                // シンタックスハイライト適用
                let highlighted_spans =
                    highlight_line(full_line_trimmed, &self.syntax_registry.theme, syntax_opt);

                // 横スクロールに応じたスライス
                let visible_spans = slice_spans(highlighted_spans, self.scroll_col, content_width);

                let mut line_spans = vec![gutter_span];
                line_spans.extend(visible_spans);

                lines.push(Line::from(line_spans));
            } else {
                let gutter_span = Span::styled(
                    format!("~{:>width$}", "", width = gutter_width - 1),
                    Style::default().fg(Color::DarkGray),
                );
                lines.push(Line::from(vec![gutter_span]));
            }
        }

        let text_widget = Paragraph::new(lines).block(Block::default());
        frame.render_widget(text_widget, text_area);

        // 2. Status Bar Rendering
        let file_name = self
            .buffer
            .path
            .as_ref()
            .map(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_else(|| "[No Name]".into());
        let mod_flag = if self.buffer.is_modified { " *" } else { "" };
        let syntax_name = syntax_opt.map(|s| s.name.as_str()).unwrap_or("Plain Text");

        let status_left = format!(" - {}{} [{}]", file_name, mod_flag, syntax_name);
        let status_right = format!("Ln {}, Col {} ", self.cursor_row + 1, self.cursor_col + 1);

        let pad_len =
            (status_area.width as usize).saturating_sub(status_left.len() + status_right.len());
        let status_text = format!("{}{:pad$}{}", status_left, "", status_right, pad = pad_len);

        let status_widget = Paragraph::new(status_text).style(
            Style::default()
                .bg(Color::Rgb(40, 44, 52))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(status_widget, status_area);

        // 3. Message Area (Echo area / Minibuffer)
        if let Some(ref prompt) = self.prompt {
            let mut spans = vec![
                Span::styled(
                    &prompt.label,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&prompt.input),
            ];

            if let Some(ref hint) = prompt.completions_hint {
                spans.push(Span::styled(
                    format!("  {}", hint),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let prompt_line = Line::from(spans);
            let prompt_widget = Paragraph::new(prompt_line);
            frame.render_widget(prompt_widget, message_area);

            // Cursor in Minibuffer prompt
            let prompt_cursor_x = message_area.x + prompt.label.len() as u16 + prompt.cursor as u16;
            if prompt_cursor_x < message_area.x + message_area.width {
                frame.set_cursor_position(Position::new(prompt_cursor_x, message_area.y));
            }
        } else {
            let msg_text = self.message.as_deref().unwrap_or("");
            let msg_widget = Paragraph::new(msg_text).style(Style::default().fg(Color::Yellow));
            frame.render_widget(msg_widget, message_area);

            // 4. Cursor Position in Text Area
            let screen_cursor_x = text_area.x
                + gutter_width as u16
                + (self.cursor_col.saturating_sub(self.scroll_col) as u16);
            let screen_cursor_y =
                text_area.y + (self.cursor_row.saturating_sub(self.scroll_row) as u16);

            if screen_cursor_x < text_area.x + text_area.width
                && screen_cursor_y < text_area.y + text_area.height
            {
                frame.set_cursor_position(Position::new(screen_cursor_x, screen_cursor_y));
            }
        }
    }
}

fn slice_spans(spans: Vec<Span<'_>>, start_col: usize, max_len: usize) -> Vec<Span<'static>> {
    let mut result = Vec::new();
    let mut current_col = 0;
    let end_col = start_col + max_len;

    for span in spans {
        let span_len = span.content.chars().count();
        let span_start = current_col;
        let span_end = current_col + span_len;

        if span_end > start_col && span_start < end_col {
            let slice_start = start_col.saturating_sub(span_start);
            let slice_end = (end_col - span_start).min(span_len);

            if slice_start < slice_end {
                let text: String = span
                    .content
                    .chars()
                    .skip(slice_start)
                    .take(slice_end - slice_start)
                    .collect();
                result.push(Span::styled(text, span.style));
            }
        }

        current_col = span_end;
        if current_col >= end_col {
            break;
        }
    }

    result
}
