use ropey::Rope;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use crate::completion::expand_tilde;

/// 単一のテキスト編集アクション（挿入または削除）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditAction {
    /// 位置 `char_idx` に文字列 `text` を挿入した
    Insert { char_idx: usize, text: String },
    /// 位置 `char_idx` から文字列 `text` を削除した
    Delete { char_idx: usize, text: String },
}

/// 1回のユーザー操作（キー入力など）に対応するUndo/Redo単位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoGroup {
    /// 実行された編集アクション一覧
    pub actions: Vec<EditAction>,
    /// 操作前のカーソル位置 (row, col)
    pub cursor_before: (usize, usize),
    /// 操作後のカーソル位置 (row, col)
    pub cursor_after: (usize, usize),
}

/// 編集対象のテキストコンテンツとファイル状態を管理するテキストバッファ。
///
/// 内部データ構造として [`ropey::Rope`] を使用しており、大容量テキストでも
/// 高速なランダムアクセスと挿入・削除操作を提供します。
pub struct Buffer {
    /// テキストの実体を保持するRopeデータ構造
    pub rope: Rope,
    /// 関連付けられているファイルパス（未保存の新規バッファの場合は `None`）
    pub path: Option<PathBuf>,
    /// バッファが変更されたかどうかのフラグ
    pub is_modified: bool,
    /// Undo用の履歴スタック
    pub undo_stack: Vec<UndoGroup>,
    /// Redo用の履歴スタック
    pub redo_stack: Vec<UndoGroup>,
    /// 前回保存時点の `undo_stack` の長さ（保存状態の判定に使用）
    pub saved_undo_len: usize,
}

impl Buffer {
    /// 空の新規バッファを作成します。
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            is_modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            saved_undo_len: 0,
        }
    }

    /// 指定されたパスのファイルを開き、バッファを構築します。
    ///
    /// チルダ（`~`）で始まるパスは自動的にホームディレクトリに展開されます。
    /// ファイルが存在しない場合は空のバッファを作成し、指定パスを保持します。
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_str = path.as_ref().to_string_lossy();
        let expanded = expand_tilde(&path_str);
        let path_buf = PathBuf::from(expanded);

        if path_buf.exists() {
            let file = File::open(&path_buf)?;
            let rope = Rope::from_reader(BufReader::new(file))?;
            Ok(Self {
                rope,
                path: Some(path_buf),
                is_modified: false,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
                saved_undo_len: 0,
            })
        } else {
            Ok(Self {
                rope: Rope::new(),
                path: Some(path_buf),
                is_modified: false,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
                saved_undo_len: 0,
            })
        }
    }

    /// 現在関連付けられているファイルパスにバッファの内容を上書き保存します。
    pub fn save(&mut self) -> io::Result<()> {
        if let Some(ref path) = self.path {
            let file = File::create(path)?;
            self.rope.write_to(BufWriter::new(file))?;
            self.saved_undo_len = self.undo_stack.len();
            self.is_modified = false;
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No file path specified",
            ))
        }
    }

    /// 指定された新しいパスにバッファの内容を保存し、バッファのパスを更新します。
    pub fn save_to<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path_str = path.as_ref().to_string_lossy();
        let expanded = expand_tilde(&path_str);
        let path_buf = PathBuf::from(expanded);

        let file = File::create(&path_buf)?;
        self.rope.write_to(BufWriter::new(file))?;
        self.path = Some(path_buf);
        self.saved_undo_len = self.undo_stack.len();
        self.is_modified = false;
        Ok(())
    }

    /// バッファの変更フラグ状態を更新します。
    fn update_modified_state(&mut self) {
        self.is_modified = self.undo_stack.len() != self.saved_undo_len;
    }

    /// Undo履歴にグループを追加し、Redoスタックをクリアします。
    fn push_undo_group(&mut self, group: UndoGroup) {
        self.redo_stack.clear();
        self.undo_stack.push(group);
        self.update_modified_state();
    }

    /// バッファの総行数を取得します（最小でも1行）。
    pub fn len_lines(&self) -> usize {
        let count = self.rope.len_lines();
        if count == 0 { 1 } else { count }
    }

    /// 指定した行インデックスの文字数（末尾の改行コードを除いた長さ）を取得します。
    pub fn line_len(&self, line_idx: usize) -> usize {
        if line_idx >= self.rope.len_lines() {
            return 0;
        }
        let line = self.rope.line(line_idx);
        let mut len = line.len_chars();
        // 末尾の改行文字分を除いた長さを返す
        if len > 0 {
            let last_char = line.char(len - 1);
            if last_char == '\n' {
                len -= 1;
                if len > 0 && line.char(len - 1) == '\r' {
                    len -= 1;
                }
            }
        }
        len
    }

    /// 指定した行・列位置に文字を挿入し、挿入後の新しいカーソル位置 `(row, col + 1)` を返します。
    pub fn insert_char(&mut self, row: usize, col: usize, c: char) -> (usize, usize) {
        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert_char(char_idx, c);
        let cursor_before = (row, col);
        let cursor_after = (row, col + 1);
        self.push_undo_group(UndoGroup {
            actions: vec![EditAction::Insert {
                char_idx,
                text: c.to_string(),
            }],
            cursor_before,
            cursor_after,
        });
        cursor_after
    }

    /// 指定した行・列位置の文字（カーソル位置の文字）を削除します。削除された場合は `true` を返します。
    pub fn delete_char(&mut self, row: usize, col: usize) -> bool {
        let char_idx = self.point_to_char_idx(row, col);
        if char_idx < self.rope.len_chars() {
            let deleted_char = self.rope.char(char_idx);
            self.rope.remove(char_idx..char_idx + 1);
            let cursor_point = (row, col);
            self.push_undo_group(UndoGroup {
                actions: vec![EditAction::Delete {
                    char_idx,
                    text: deleted_char.to_string(),
                }],
                cursor_before: cursor_point,
                cursor_after: cursor_point,
            });
            true
        } else {
            false
        }
    }

    /// 指定した行・列位置の直前の文字（バックスペース）を削除し、
    /// 削除後の新しい (row, col) 位置を返します。
    pub fn delete_backspace(&mut self, row: usize, col: usize) -> Option<(usize, usize)> {
        let char_idx = self.point_to_char_idx(row, col);
        if char_idx > 0 {
            let deleted_char = self.rope.char(char_idx - 1);
            self.rope.remove(char_idx - 1..char_idx);
            let new_char_idx = char_idx - 1;
            let new_row = self.rope.char_to_line(new_char_idx);
            let line_start_char = self.rope.line_to_char(new_row);
            let new_col = new_char_idx - line_start_char;
            let cursor_before = (row, col);
            let cursor_after = (new_row, new_col);
            self.push_undo_group(UndoGroup {
                actions: vec![EditAction::Delete {
                    char_idx: new_char_idx,
                    text: deleted_char.to_string(),
                }],
                cursor_before,
                cursor_after,
            });
            Some((new_row, new_col))
        } else {
            None
        }
    }

    /// 指定した行・列位置に改行を挿入し、新しい (row, col) 位置 `(row + 1, 0)` を返します。
    pub fn insert_newline(&mut self, row: usize, col: usize) -> (usize, usize) {
        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert_char(char_idx, '\n');
        let cursor_before = (row, col);
        let cursor_after = (row + 1, 0);
        self.push_undo_group(UndoGroup {
            actions: vec![EditAction::Insert {
                char_idx,
                text: "\n".to_string(),
            }],
            cursor_before,
            cursor_after,
        });
        cursor_after
    }

    /// 指定した行・列位置から指定文字数分の範囲を削除します（行末削除用）。
    pub fn delete_range(&mut self, row: usize, col: usize, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let char_idx = self.point_to_char_idx(row, col);
        if char_idx + len <= self.rope.len_chars() {
            let deleted_slice = self.rope.slice(char_idx..char_idx + len);
            let deleted_text = deleted_slice.to_string();
            self.rope.remove(char_idx..char_idx + len);
            let cursor_point = (row, col);
            self.push_undo_group(UndoGroup {
                actions: vec![EditAction::Delete {
                    char_idx,
                    text: deleted_text,
                }],
                cursor_before: cursor_point,
                cursor_after: cursor_point,
            });
            true
        } else {
            false
        }
    }

    /// 指定した2点間の選択範囲（リージョン）のテキストを取得します。
    pub fn get_region_text(&self, mark: (usize, usize), point: (usize, usize)) -> String {
        let idx1 = self.point_to_char_idx(mark.0, mark.1);
        let idx2 = self.point_to_char_idx(point.0, point.1);
        let from = idx1.min(idx2);
        let to = idx1.max(idx2);
        if from == to {
            String::new()
        } else {
            self.rope.slice(from..to).to_string()
        }
    }

    /// 指定した2点間の選択範囲（リージョン）を削除し、削除後の新しいカーソル位置 `(row, col)` を返します。
    pub fn delete_region(&mut self, mark: (usize, usize), point: (usize, usize)) -> (usize, usize) {
        let idx1 = self.point_to_char_idx(mark.0, mark.1);
        let idx2 = self.point_to_char_idx(point.0, point.1);
        let from = idx1.min(idx2);
        let to = idx1.max(idx2);
        if from == to {
            return point;
        }

        let deleted_slice = self.rope.slice(from..to);
        let deleted_text = deleted_slice.to_string();
        self.rope.remove(from..to);

        let new_cursor = self.char_idx_to_point(from);
        self.push_undo_group(UndoGroup {
            actions: vec![EditAction::Delete {
                char_idx: from,
                text: deleted_text,
            }],
            cursor_before: point,
            cursor_after: new_cursor,
        });

        new_cursor
    }

    /// 指定した行・列位置に文字列を挿入し、挿入後の新しいカーソル位置 `(row, col)` を返します。
    pub fn insert_str(&mut self, row: usize, col: usize, text: &str) -> (usize, usize) {
        if text.is_empty() {
            return (row, col);
        }

        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert(char_idx, text);

        let new_char_idx = char_idx + text.chars().count();
        let cursor_after = self.char_idx_to_point(new_char_idx);

        self.push_undo_group(UndoGroup {
            actions: vec![EditAction::Insert {
                char_idx,
                text: text.to_string(),
            }],
            cursor_before: (row, col),
            cursor_after,
        });

        cursor_after
    }

    /// 直前の操作を取り消し（Undo）、移動すべきカーソル位置 `(row, col)` を返します。
    pub fn undo(&mut self) -> Option<(usize, usize)> {
        let group = self.undo_stack.pop()?;

        // 逆順でアクションの逆操作を適用
        for action in group.actions.iter().rev() {
            match action {
                EditAction::Insert { char_idx, text } => {
                    let len = text.chars().count();
                    self.rope.remove(*char_idx..*char_idx + len);
                }
                EditAction::Delete { char_idx, text } => {
                    self.rope.insert(*char_idx, text);
                }
            }
        }

        let target_cursor = group.cursor_before;
        self.redo_stack.push(group);
        self.update_modified_state();
        Some(target_cursor)
    }

    /// 取り消した操作を再実行（Redo）、移動すべきカーソル位置 `(row, col)` を返します。
    pub fn redo(&mut self) -> Option<(usize, usize)> {
        let group = self.redo_stack.pop()?;

        // 順方向でアクションを適用
        for action in &group.actions {
            match action {
                EditAction::Insert { char_idx, text } => {
                    self.rope.insert(*char_idx, text);
                }
                EditAction::Delete { char_idx, text } => {
                    let len = text.chars().count();
                    self.rope.remove(*char_idx..*char_idx + len);
                }
            }
        }

        let target_cursor = group.cursor_after;
        self.undo_stack.push(group);
        self.update_modified_state();
        Some(target_cursor)
    }

    /// 行・列の座標 `(row, col)` を、Rope全体の絶対文字インデックスに変換します。
    pub fn point_to_char_idx(&self, row: usize, col: usize) -> usize {
        let line_count = self.rope.len_lines();
        if line_count == 0 {
            return 0;
        }
        let clamped_row = row.min(line_count - 1);
        let line_start = self.rope.line_to_char(clamped_row);
        let max_col = self.line_len(clamped_row);
        let clamped_col = col.min(max_col);
        (line_start + clamped_col).min(self.rope.len_chars())
    }

    /// 絶対文字インデックス `char_idx` を、行・列の座標 `(row, col)` に逆変換します。
    pub fn char_idx_to_point(&self, char_idx: usize) -> (usize, usize) {
        let clamped_idx = char_idx.min(self.rope.len_chars());
        let row = self.rope.char_to_line(clamped_idx);
        let line_start = self.rope.line_to_char(row);
        let col = clamped_idx - line_start;
        (row, col)
    }

    /// 指定した行の文字列（末尾の改行コードを除いたもの）を取得します。
    pub fn line_trimmed_string(&self, line_idx: usize) -> String {
        if line_idx >= self.rope.len_lines() {
            return String::new();
        }
        let full_str = self.rope.line(line_idx).to_string();
        full_str.trim_end_matches(['\r', '\n']).to_string()
    }

    /// 指定した開始座標 `(start_row, start_col)` から順方向（前方から末尾へ）に `query` を検索します。
    ///
    /// - `inclusive` が `true` の場合、`start_col` 自体から始まるマッチも含めます。
    /// - `inclusive` が `false` の場合、`start_col` より後ろから始まるマッチを検索します。
    /// - 末尾まで見つからない場合はファイル先頭へ折り返して検索し、第2戻り値（`wrapped`）を `true` にします。
    pub fn search_forward(
        &self,
        query: &str,
        start_row: usize,
        start_col: usize,
        inclusive: bool,
    ) -> Option<((usize, usize), bool)> {
        if query.is_empty() {
            return None;
        }
        let total_lines = self.len_lines();
        if total_lines == 0 {
            return None;
        }
        let start_row = start_row.min(total_lines - 1);

        // 1. 現在位置からバッファ末尾まで検索
        for row in start_row..total_lines {
            let line_str = self.line_trimmed_string(row);
            let matches = find_matches_in_line(&line_str, query);
            for col in matches {
                if row > start_row
                    || (inclusive && col >= start_col)
                    || (!inclusive && col > start_col)
                {
                    return Some(((row, col), false));
                }
            }
        }

        // 2. バッファ先頭から現在位置まで折り返し（ラップアラウンド）検索
        for row in 0..=start_row {
            let line_str = self.line_trimmed_string(row);
            let matches = find_matches_in_line(&line_str, query);
            for col in matches {
                if row < start_row
                    || (inclusive && col < start_col)
                    || (!inclusive && col <= start_col)
                {
                    return Some(((row, col), true));
                }
            }
        }

        None
    }

    /// 指定した開始座標 `(start_row, start_col)` から逆方向（後方から先頭へ）に `query` を検索します。
    ///
    /// - `inclusive` が `true` の場合、`start_col` 自体から始まるマッチも含めます。
    /// - `inclusive` が `false` の場合、`start_col` より前から始まるマッチを検索します。
    /// - 先頭まで見つからない場合はファイル末尾へ折り返して検索し、第2戻り値（`wrapped`）を `true` にします。
    pub fn search_backward(
        &self,
        query: &str,
        start_row: usize,
        start_col: usize,
        inclusive: bool,
    ) -> Option<((usize, usize), bool)> {
        if query.is_empty() {
            return None;
        }
        let total_lines = self.len_lines();
        if total_lines == 0 {
            return None;
        }
        let start_row = start_row.min(total_lines - 1);

        // 1. 現在位置からバッファ先頭まで逆順検索
        for row in (0..=start_row).rev() {
            let line_str = self.line_trimmed_string(row);
            let matches = find_matches_in_line(&line_str, query);
            for col in matches.into_iter().rev() {
                if row < start_row
                    || (inclusive && col <= start_col)
                    || (!inclusive && col < start_col)
                {
                    return Some(((row, col), false));
                }
            }
        }

        // 2. バッファ末尾から現在位置まで折り返し（ラップアラウンド）逆順検索
        for row in (start_row..total_lines).rev() {
            let line_str = self.line_trimmed_string(row);
            let matches = find_matches_in_line(&line_str, query);
            for col in matches.into_iter().rev() {
                if row > start_row
                    || (inclusive && col > start_col)
                    || (!inclusive && col >= start_col)
                {
                    return Some(((row, col), true));
                }
            }
        }

        None
    }
}

/// 2つの文字が一致するかを判定します（`case_sensitive` が `false` の場合は大文字・小文字を区別しません）。
#[inline]
fn chars_match(a: char, b: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        a == b
    } else if a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(&b)
    } else {
        a.to_lowercase().eq(b.to_lowercase())
    }
}

/// 指定した1行の文字列（改行除く）から、検索クエリに一致するすべての開始列インデックス（文字単位）を返します。
///
/// クエリに大文字が含まれる場合は大文字・小文字を区別し、すべて小文字の場合は区別しないスマートケース検索を行います。
pub fn find_matches_in_line(line: &str, query: &str) -> Vec<usize> {
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return Vec::new();
    }
    let line_chars: Vec<char> = line.chars().collect();
    let q_len = query_chars.len();
    if q_len > line_chars.len() {
        return Vec::new();
    }

    let case_sensitive = query_chars.iter().any(|c| c.is_uppercase());
    let mut matches = Vec::new();

    for start_col in 0..=(line_chars.len() - q_len) {
        let mut matched = true;
        for i in 0..q_len {
            if !chars_match(line_chars[start_col + i], query_chars[i], case_sensitive) {
                matched = false;
                break;
            }
        }
        if matched {
            matches.push(start_col);
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_undo_redo() {
        let mut buf = Buffer::new();
        assert!(!buf.is_modified);

        buf.insert_char(0, 0, 'a');
        buf.insert_char(0, 1, 'b');
        buf.insert_char(0, 2, 'c');
        assert_eq!(buf.rope.to_string(), "abc");
        assert!(buf.is_modified);

        // Undo 'c'
        let pos = buf.undo();
        assert_eq!(pos, Some((0, 2)));
        assert_eq!(buf.rope.to_string(), "ab");

        // Undo 'b'
        let pos = buf.undo();
        assert_eq!(pos, Some((0, 1)));
        assert_eq!(buf.rope.to_string(), "a");

        // Redo 'b'
        let pos = buf.redo();
        assert_eq!(pos, Some((0, 2)));
        assert_eq!(buf.rope.to_string(), "ab");

        // Redo 'c'
        let pos = buf.redo();
        assert_eq!(pos, Some((0, 3)));
        assert_eq!(buf.rope.to_string(), "abc");
    }

    #[test]
    fn test_delete_and_undo_redo() {
        let mut buf = Buffer::new();
        buf.rope.append(Rope::from_str("hello"));
        buf.saved_undo_len = 0;

        buf.delete_backspace(0, 5); // deletes 'o'
        assert_eq!(buf.rope.to_string(), "hell");

        let pos = buf.undo();
        assert_eq!(pos, Some((0, 5)));
        assert_eq!(buf.rope.to_string(), "hello");
        assert!(!buf.is_modified); // initial state restored

        let pos = buf.redo();
        assert_eq!(pos, Some((0, 4)));
        assert_eq!(buf.rope.to_string(), "hell");
        assert!(buf.is_modified);
    }

    #[test]
    fn test_newline_and_range_undo() {
        let mut buf = Buffer::new();
        buf.insert_char(0, 0, 'x');
        buf.insert_newline(0, 1);
        buf.insert_char(1, 0, 'y');
        assert_eq!(buf.rope.to_string(), "x\ny");

        buf.undo(); // undo 'y'
        buf.undo(); // undo newline
        assert_eq!(buf.rope.to_string(), "x");

        buf.delete_range(0, 0, 1);
        assert_eq!(buf.rope.to_string(), "");

        buf.undo(); // undo delete_range
        assert_eq!(buf.rope.to_string(), "x");
    }

    #[test]
    fn test_region_get_delete_undo() {
        let mut buf = Buffer::new();
        buf.insert_str(0, 0, "Hello, World!");
        assert_eq!(buf.rope.to_string(), "Hello, World!");

        let text = buf.get_region_text((0, 7), (0, 12));
        assert_eq!(text, "World");

        let pos = buf.delete_region((0, 7), (0, 12));
        assert_eq!(pos, (0, 7));
        assert_eq!(buf.rope.to_string(), "Hello, !");

        let pos = buf.undo();
        assert_eq!(pos, Some((0, 12)));
        assert_eq!(buf.rope.to_string(), "Hello, World!");
    }

    #[test]
    fn test_insert_str_and_undo() {
        let mut buf = Buffer::new();
        buf.insert_str(0, 0, "Line 1\nLine 2");
        assert_eq!(buf.rope.to_string(), "Line 1\nLine 2");

        let pos = buf.undo();
        assert_eq!(pos, Some((0, 0)));
        assert_eq!(buf.rope.to_string(), "");
    }

    #[test]
    fn test_find_matches_in_line_smart_case_and_unicode() {
        // 小文字クエリは大文字・小文字を区別しない (Smart-case)
        assert_eq!(
            find_matches_in_line("Hello hello HELLO", "hello"),
            vec![0, 6, 12]
        );
        // 大文字を含むクエリは大文字・小文字を区別する
        assert_eq!(find_matches_in_line("Hello hello HELLO", "Hello"), vec![0]);
        // 日本語（マルチバイトUTF-8）でも文字インデックスで返る
        assert_eq!(
            find_matches_in_line("あいうえお_あい_かきく", "あい"),
            vec![0, 6]
        );
    }

    #[test]
    fn test_search_forward_and_backward() {
        let mut buf = Buffer::new();
        buf.insert_str(0, 0, "apple banana\ncherry apple\nbanana date");

        // (0, 0) から "apple" を inclusive=true で前方検索 -> (0, 0)
        assert_eq!(
            buf.search_forward("apple", 0, 0, true),
            Some(((0, 0), false))
        );
        // (0, 0) から "apple" を inclusive=false で前方検索 -> 次の行の (1, 7)
        assert_eq!(
            buf.search_forward("apple", 0, 0, false),
            Some(((1, 7), false))
        );
        // (1, 7) から "apple" を inclusive=false で前方検索 -> ラップアラウンドして (0, 0)
        assert_eq!(
            buf.search_forward("apple", 1, 7, false),
            Some(((0, 0), true))
        );

        // (2, 10) から "banana" を inclusive=true で後方検索 -> (2, 0)
        assert_eq!(
            buf.search_backward("banana", 2, 10, true),
            Some(((2, 0), false))
        );
        // (2, 0) から "banana" を inclusive=false で後方検索 -> (0, 6)
        assert_eq!(
            buf.search_backward("banana", 2, 0, false),
            Some(((0, 6), false))
        );
        // (0, 6) から "banana" を inclusive=false で後方検索 -> ラップアラウンドして (2, 0)
        assert_eq!(
            buf.search_backward("banana", 0, 6, false),
            Some(((2, 0), true))
        );
    }
}
