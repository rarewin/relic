use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::path::{Path, PathBuf};

use crate::buffer::Buffer;
use crate::completion::{complete_path, expand_tilde};
use crate::syntax::{SyntaxRegistry, highlight_line};

/// エディタのプレフィックスキー入力状態。
#[derive(Debug, PartialEq, Eq)]
pub enum PrefixKey {
    /// プレフィックスなしの通常状態
    None,
    /// `Ctrl+x` が押された後の状態
    CtrlX,
    /// `Esc` (Meta) が押された後の状態
    Meta,
}

/// ミニバッファプロンプトの種類。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptType {
    /// ファイル名指定での保存プロンプト（`Save file as: ` / `Write file: `）
    SaveAs,
    /// 既存ファイル上書き確認ダイアログ
    ConfirmOverwrite(String),
    /// ファイルオープンプロンプト（`Find file: `）
    FindFile,
    /// 未保存変更がある場合の終了確認プロンプト
    ConfirmQuit,
}

/// ミニバッファでのプロンプト入力状態と補完ステート。
#[derive(Debug, Clone)]
pub struct PromptState {
    /// プロンプトの種別
    pub prompt_type: PromptType,
    /// 画面左側に表示されるプロンプトラベル（例: `"Find file: "`）
    pub label: String,
    /// ユーザーが入力中の文字列
    pub input: String,
    /// 入力文字列内のカーソル位置（文字単位）
    pub cursor: usize,
    /// プロンプト右側に表示される補完候補ヒント文字列
    pub completions_hint: Option<String>,
    /// 補完時の基準ディレクトリプレフィックス（例: `"src/"`）
    pub completion_base_dir: Option<String>,
    /// マッチした補完候補の一覧
    pub completion_candidates: Vec<String>,
    /// サイクル補完で現在選択中の候補インデックス
    pub completion_index: Option<usize>,
}

/// テキストエディタの全体状態を管理するメイン構造体。
pub struct Editor {
    /// 編集中のテキストバッファ
    pub buffer: Buffer,
    /// バッファ内のカーソル行位置（0-indexed）
    pub cursor_row: usize,
    /// バッファ内のカーソル列位置（0-indexed, 文字単位）
    pub cursor_col: usize,
    /// 画面上端に表示されているバッファの開始行（スクロールオフセット）
    pub scroll_row: usize,
    /// 画面左端に表示されているバッファの開始列（横スクロールオフセット）
    pub scroll_col: usize,
    /// エディタ描画領域の行数（高さ）
    pub page_height: usize,
    /// 現在のプレフィックスキー入力状態
    pub prefix: PrefixKey,
    /// アクティブなミニバッファプロンプト（存在する場合）
    pub prompt: Option<PromptState>,
    /// 最下行エコーエリアに表示する通知メッセージ
    pub message: Option<String>,
    /// エディタの終了フラグ
    pub should_quit: bool,
    /// 構文ハイライトとテーマのレジストリ
    pub syntax_registry: SyntaxRegistry,
}

impl Editor {
    /// 指定された [`Buffer`] を保持する新しい [`Editor`] インスタンスを初期化します。
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            page_height: 24,
            prefix: PrefixKey::None,
            prompt: None,
            message: Some(
                "Welcome to Relic! (C-x C-f: Find file, C-x C-s: Save, C-x C-w: Write file, C-x C-c: Quit)".into(),
            ),
            should_quit: false,
            syntax_registry: SyntaxRegistry::new(),
        }
    }

    /// キー入力イベントを受け取り、プロンプト入力または通常のエディタ操作へディスパッチします。
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let is_alt = key.modifiers.contains(KeyModifiers::ALT);

        // 1. If currently inside a Minibuffer prompt
        if self.prompt.is_some() {
            self.handle_prompt_key(key, is_ctrl);
            return;
        }

        // 2. Normal Editor key handling
        match self.prefix {
            PrefixKey::None => {
                // ASCII 0x1F (Unit Separator = Ctrl+/ or Ctrl+_)
                if key.code == KeyCode::Char('\x1f') || key.code == KeyCode::Char('\u{001f}') {
                    self.undo();
                    return;
                }

                if is_alt {
                    match key.code {
                        KeyCode::Char('v') => self.page_up(),
                        KeyCode::Char('_') | KeyCode::Char('-') => self.redo(),
                        _ => {}
                    }
                } else if is_ctrl {
                    match key.code {
                        // Undo: ターミナルVT100規格では ASCII 0x1F (C-/, C-_, C-7) が Undo に対応
                        // (Crossterm は 0x1F を Char('7') + CONTROL または Char('_') としてデコードする)
                        KeyCode::Char('/')
                        | KeyCode::Char('_')
                        | KeyCode::Char('7')
                        | KeyCode::Char('\x1f') => self.undo(),

                        // Emacs basic navigation & editing
                        KeyCode::Char('n') => self.move_down(),
                        KeyCode::Char('p') => self.move_up(),
                        KeyCode::Char('f') => self.move_right(),
                        KeyCode::Char('b') => self.move_left(),
                        KeyCode::Char('a') => self.move_to_bol(),
                        KeyCode::Char('e') => self.move_to_eol(),
                        KeyCode::Char('v') => self.page_down(),
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
                        _ => {
                            self.message = Some(format!(
                                "C-{} is undefined",
                                match key.code {
                                    KeyCode::Char(c) => c.to_string(),
                                    _ => "?".into(),
                                }
                            ));
                        }
                    }
                } else {
                    match key.code {
                        KeyCode::Up => self.move_up(),
                        KeyCode::Down => self.move_down(),
                        KeyCode::Left => self.move_left(),
                        KeyCode::Right => self.move_right(),
                        KeyCode::Home => self.move_to_bol(),
                        KeyCode::End => self.move_to_eol(),
                        KeyCode::PageDown => self.page_down(),
                        KeyCode::PageUp => self.page_up(),
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
                            self.prefix = PrefixKey::Meta;
                            self.message = Some("ESC-".into());
                        }
                        _ => {}
                    }
                }
            }
            PrefixKey::CtrlX => {
                self.prefix = PrefixKey::None;
                if is_ctrl {
                    match key.code {
                        KeyCode::Char('f') => self.request_find_file(),
                        KeyCode::Char('s') => self.request_save(),
                        KeyCode::Char('w') => self.request_write_file(),
                        KeyCode::Char('u') => self.undo(),
                        KeyCode::Char('c') => self.request_quit(),
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
                        KeyCode::Char('f') => self.request_find_file(),
                        KeyCode::Char('s') => self.request_save(),
                        KeyCode::Char('w') => self.request_write_file(),
                        KeyCode::Char('u') => self.undo(),
                        KeyCode::Char('c') => self.request_quit(),
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
            PrefixKey::Meta => {
                self.prefix = PrefixKey::None;
                match key.code {
                    KeyCode::Char('v') => self.page_up(),
                    KeyCode::Char('_') | KeyCode::Char('-') => self.redo(),
                    KeyCode::Char('g') | KeyCode::Esc => {
                        self.message = Some("Quit".into());
                    }
                    _ => {
                        self.message = Some(format!(
                            "ESC {} is undefined",
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

    /// ミニバッファプロンプト表示中のキー入力を処理します。
    fn handle_prompt_key(&mut self, key: KeyEvent, is_ctrl: bool) {
        let prompt_type = self.prompt.as_ref().map(|p| p.prompt_type.clone());

        match prompt_type {
            Some(PromptType::ConfirmQuit) => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.prompt = None;
                    self.should_quit = true;
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
            },
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
            Some(PromptType::SaveAs) | Some(PromptType::FindFile) => {
                if is_ctrl {
                    match key.code {
                        KeyCode::Char('g') => {
                            self.prompt = None;
                            self.message = Some("Quit".into());
                        }
                        KeyCode::Char('h') => {
                            self.prompt_backspace();
                        }
                        KeyCode::Char('i') => {
                            // Ctrl+I is Tab
                            self.prompt_complete(false);
                        }
                        KeyCode::Char('n')
                            if self
                                .prompt
                                .as_ref()
                                .and_then(|p| p.completion_index)
                                .is_some() =>
                        {
                            self.prompt_complete(false);
                        }
                        KeyCode::Char('p')
                            if self
                                .prompt
                                .as_ref()
                                .and_then(|p| p.completion_index)
                                .is_some() =>
                        {
                            self.prompt_complete(true);
                        }
                        KeyCode::Char('a') => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = 0;
                                p.completions_hint = None;
                                p.completion_index = None;
                            }
                        }
                        KeyCode::Char('e') => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = p.input.chars().count();
                                p.completions_hint = None;
                                p.completion_index = None;
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
                            self.prompt_complete(false);
                        }
                        KeyCode::BackTab => {
                            self.prompt_complete(true);
                        }
                        KeyCode::Down
                            if self
                                .prompt
                                .as_ref()
                                .and_then(|p| p.completion_index)
                                .is_some() =>
                        {
                            self.prompt_complete(false);
                        }
                        KeyCode::Up
                            if self
                                .prompt
                                .as_ref()
                                .and_then(|p| p.completion_index)
                                .is_some() =>
                        {
                            self.prompt_complete(true);
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
                                p.completion_index = None;
                            }
                        }
                        KeyCode::End => {
                            if let Some(ref mut p) = self.prompt {
                                p.cursor = p.input.chars().count();
                                p.completions_hint = None;
                                p.completion_index = None;
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

    /// ミニバッファでのTab補完および候補の順繰りサイクル選択を実行します。
    fn prompt_complete(&mut self, reverse: bool) {
        let Some(ref mut p) = self.prompt else {
            return;
        };

        if !p.completion_candidates.is_empty()
            && let Some(idx) = p.completion_index
        {
            // 既に補完サイクル中の場合：次/前の候補へ進む
            let total = p.completion_candidates.len();
            let next_idx = if reverse {
                if idx == 0 { total - 1 } else { idx - 1 }
            } else {
                (idx + 1) % total
            };

            let base_dir = p.completion_base_dir.as_deref().unwrap_or("");
            p.input = format!("{}{}", base_dir, p.completion_candidates[next_idx]);
            p.cursor = p.input.chars().count();
            p.completion_index = Some(next_idx);

            let preview = format_candidates_hint(&p.completion_candidates, next_idx);
            p.completions_hint = Some(preview);
            return;
        }

        // 初回補完
        let res = complete_path(&p.input);
        if res.candidates.is_empty() {
            p.completions_hint = Some("[No match]".into());
            p.completion_candidates.clear();
            p.completion_index = None;
            return;
        }

        if res.candidates.len() == 1 {
            if let Some(completed) = res.completed {
                p.input = completed;
                p.cursor = p.input.chars().count();
            }
            p.completions_hint = Some("[Sole completion]".into());
            p.completion_candidates.clear();
            p.completion_index = None;
            return;
        }

        // 複数候補がある場合
        if let Some(completed) = res.completed {
            // LCP で文字列が進む場合
            p.input = completed;
            p.cursor = p.input.chars().count();
            let preview = res.candidates.join("  ");
            p.completions_hint = Some(format!("[{}]", preview));
            p.completion_base_dir = Some(res.raw_dir_part);
            p.completion_candidates = res.candidates;
            p.completion_index = None;
        } else {
            // LCP では進まない（または既に最長）場合 -> 最初の候補を選択してサイクル開始
            let first_idx = 0;
            let base_dir = &res.raw_dir_part;
            p.input = format!("{}{}", base_dir, res.candidates[first_idx]);
            p.cursor = p.input.chars().count();
            p.completion_base_dir = Some(res.raw_dir_part);
            p.completion_index = Some(first_idx);
            let preview = format_candidates_hint(&res.candidates, first_idx);
            p.completions_hint = Some(preview);
            p.completion_candidates = res.candidates;
        }
    }

    /// ミニバッファプロンプト内のカーソルを1文字左へ移動します。
    fn prompt_move_left(&mut self) {
        if let Some(ref mut p) = self.prompt {
            p.cursor = p.cursor.saturating_sub(1);
            p.completions_hint = None;
            p.completion_index = None;
        }
    }

    /// ミニバッファプロンプト内のカーソルを1文字右へ移動します。
    fn prompt_move_right(&mut self) {
        if let Some(ref mut p) = self.prompt {
            let max = p.input.chars().count();
            if p.cursor < max {
                p.cursor += 1;
            }
            p.completions_hint = None;
            p.completion_index = None;
        }
    }

    /// ミニバッファプロンプトに1文字挿入します。
    fn prompt_insert_char(&mut self, c: char) {
        if let Some(ref mut p) = self.prompt {
            let mut chars: Vec<char> = p.input.chars().collect();
            chars.insert(p.cursor, c);
            p.input = chars.into_iter().collect();
            p.cursor += 1;
            p.completions_hint = None;
            p.completion_index = None;
            p.completion_candidates.clear();
        }
    }

    /// ミニバッファプロンプトでカーソル直前の1文字を削除します。
    fn prompt_backspace(&mut self) {
        if let Some(ref mut p) = self.prompt
            && p.cursor > 0
        {
            let mut chars: Vec<char> = p.input.chars().collect();
            chars.remove(p.cursor - 1);
            p.input = chars.into_iter().collect();
            p.cursor -= 1;
            p.completions_hint = None;
            p.completion_index = None;
            p.completion_candidates.clear();
        }
    }

    /// ミニバッファプロンプトでEnterが押されたときに、入力された内容を確定して対応アクションを実行します。
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
                            completion_base_dir: None,
                            completion_candidates: Vec::new(),
                            completion_index: None,
                        });
                    } else {
                        self.do_save_to(trimmed);
                    }
                }
                PromptType::FindFile => {
                    let trimmed = prompt.input.trim();
                    if trimmed.is_empty() {
                        self.message = Some("No file name specified".into());
                        return;
                    }

                    let expanded = expand_tilde(trimmed);
                    let target_path = PathBuf::from(&expanded);

                    match Buffer::open(&target_path) {
                        Ok(new_buf) => {
                            let is_new = !target_path.exists();
                            self.buffer = new_buf;
                            self.cursor_row = 0;
                            self.cursor_col = 0;
                            self.scroll_row = 0;
                            self.scroll_col = 0;
                            if is_new {
                                self.message = Some(format!("(New file) {}", trimmed));
                            } else {
                                self.message = Some(format!("Read {}", trimmed));
                            }
                        }
                        Err(e) => {
                            self.message = Some(format!("Error opening file: {}", e));
                        }
                    }
                }
                PromptType::ConfirmOverwrite(_) | PromptType::ConfirmQuit => {}
            }
        }
    }

    /// 指定パスへのバッファ保存を実行し、結果メッセージを設定します。
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

    /// エディタの終了を要求します。未保存の変更がある場合は確認プロンプトを表示します。
    pub fn request_quit(&mut self) {
        if self.buffer.is_modified {
            self.prompt = Some(PromptState {
                prompt_type: PromptType::ConfirmQuit,
                label: "Modified buffer has unsaved changes. Quit anyway? (y or n) ".into(),
                input: String::new(),
                cursor: 0,
                completions_hint: None,
                completion_base_dir: None,
                completion_candidates: Vec::new(),
                completion_index: None,
            });
            self.message = None;
        } else {
            self.should_quit = true;
        }
    }

    /// ファイルオープンプロンプト（`Find file: `）を開始します。
    pub fn request_find_file(&mut self) {
        let default_input = if let Some(ref p) = self.buffer.path {
            if let Some(parent) = p.parent() {
                let s = parent.to_string_lossy();
                if s.is_empty() || s == "." {
                    String::new()
                } else {
                    format!("{}/", s)
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let len = default_input.chars().count();
        self.prompt = Some(PromptState {
            prompt_type: PromptType::FindFile,
            label: "Find file: ".into(),
            input: default_input,
            cursor: len,
            completions_hint: None,
            completion_base_dir: None,
            completion_candidates: Vec::new(),
            completion_index: None,
        });
        self.message = None;
    }

    /// ファイル保存を要求します。ファイルパスが未設定の場合は `Save file as: ` プロンプトを開きます。
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
                completion_base_dir: None,
                completion_candidates: Vec::new(),
                completion_index: None,
            });
            self.message = None;
        }
    }

    /// 名前を付けて保存プロンプト（`Write file: `）を開始します。
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
            completion_base_dir: None,
            completion_candidates: Vec::new(),
            completion_index: None,
        });
        self.message = None;
    }

    /// 現在のファイルパスにバッファを保存します。
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

    /// カーソル位置に1文字挿入します。
    pub fn insert_char(&mut self, c: char) {
        let (new_row, new_col) = self.buffer.insert_char(self.cursor_row, self.cursor_col, c);
        self.cursor_row = new_row;
        self.cursor_col = new_col;
        self.message = None;
    }

    /// カーソル位置に改行を挿入し、カーソルを次行の先頭へ移動します。
    pub fn insert_newline(&mut self) {
        let (new_row, new_col) = self.buffer.insert_newline(self.cursor_row, self.cursor_col);
        self.cursor_row = new_row;
        self.cursor_col = new_col;
        self.message = None;
    }

    /// カーソル直前の文字を削除（バックスペース）します。
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

    /// カーソル位置の文字を削除（Delete）します。
    pub fn delete_char(&mut self) {
        self.buffer.delete_char(self.cursor_row, self.cursor_col);
        self.message = None;
    }

    /// カーソル位置から行末までの文字列を削除（Kill line）します。
    /// 行末にカーソルがある場合は改行を削除して次行を連結します。
    pub fn kill_line(&mut self) {
        let current_line_len = self.buffer.line_len(self.cursor_row);
        if self.cursor_col >= current_line_len {
            self.delete_char();
        } else {
            let delete_len = current_line_len - self.cursor_col;
            self.buffer
                .delete_range(self.cursor_row, self.cursor_col, delete_len);
        }
        self.message = None;
    }

    /// 直前の編集操作を取り消します（Undo）。
    pub fn undo(&mut self) {
        if let Some((new_row, new_col)) = self.buffer.undo() {
            self.cursor_row = new_row;
            self.cursor_col = new_col;
            self.clamp_cursor_col();
            self.message = Some("Undo!".into());
        } else {
            self.message = Some("No further undo information".into());
        }
    }

    /// 取り消した編集操作を再実行します（Redo）。
    pub fn redo(&mut self) {
        if let Some((new_row, new_col)) = self.buffer.redo() {
            self.cursor_row = new_row;
            self.cursor_col = new_col;
            self.clamp_cursor_col();
            self.message = Some("Redo!".into());
        } else {
            self.message = Some("No further redo information".into());
        }
    }

    /// カーソルを1行上へ移動します。
    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor_col();
        }
        self.message = None;
    }

    /// カーソルを1行下へ移動します。
    pub fn move_down(&mut self) {
        let total_lines = self.buffer.len_lines();
        if self.cursor_row + 1 < total_lines {
            self.cursor_row += 1;
            self.clamp_cursor_col();
        }
        self.message = None;
    }

    /// カーソルを1文字左へ移動します（行頭の場合は前行の行末へ移動）。
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.buffer.line_len(self.cursor_row);
        }
        self.message = None;
    }

    /// カーソルを1文字右へ移動します（行末の場合は次行の行頭へ移動）。
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

    /// カーソルを現在の行頭へ移動します。
    pub fn move_to_bol(&mut self) {
        self.cursor_col = 0;
        self.message = None;
    }

    /// カーソルを現在の行末へ移動します。
    pub fn move_to_eol(&mut self) {
        self.cursor_col = self.buffer.line_len(self.cursor_row);
        self.message = None;
    }

    /// 1画面分下へスクロールし、カーソルを移動します（Page Down / C-v）。
    pub fn page_down(&mut self) {
        let step = self.page_height.saturating_sub(2).max(1);
        let total = self.buffer.len_lines();
        if self.cursor_row + step < total {
            self.cursor_row += step;
        } else {
            self.cursor_row = total.saturating_sub(1);
        }
        self.clamp_cursor_col();
        self.message = None;
    }

    /// 1画面分上へスクロールし、カーソルを移動します（Page Up / M-v）。
    pub fn page_up(&mut self) {
        let step = self.page_height.saturating_sub(2).max(1);
        self.cursor_row = self.cursor_row.saturating_sub(step);
        self.clamp_cursor_col();
        self.message = None;
    }

    /// 現在の行の長さを超えないようカーソル列座標を制限します。
    fn clamp_cursor_col(&mut self) {
        let max_col = self.buffer.line_len(self.cursor_row);
        if self.cursor_col > max_col {
            self.cursor_col = max_col;
        }
    }

    /// カーソルが画面内に収まるように画面の縦横スクロールオフセットを更新します。
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

    /// エディタ画面全体（テキスト領域・行番号・ステータスバー・ミニバッファ）をレンダリングします。
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

        self.page_height = content_height;
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

/// ハイライト済みSpanのリストを、横スクロールの開始列と表示幅に合わせてスライスします。
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

/// 補完候補一覧と現在選択中のインデックスから、ヒントプレビュー文字列（例: `[1/3] *main.rs*  buffer.rs`）を生成します。
fn format_candidates_hint(candidates: &[String], current_idx: usize) -> String {
    let mut parts = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        if i == current_idx {
            parts.push(format!("*{}*", c));
        } else {
            parts.push(c.clone());
        }
    }
    format!(
        "[{}/{}] {}",
        current_idx + 1,
        candidates.len(),
        parts.join("  ")
    )
}
