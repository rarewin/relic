use ropey::Rope;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use crate::completion::expand_tilde;

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
}

impl Buffer {
    /// 空の新規バッファを作成します。
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            is_modified: false,
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
            })
        } else {
            Ok(Self {
                rope: Rope::new(),
                path: Some(path_buf),
                is_modified: false,
            })
        }
    }

    /// 現在関連付けられているファイルパスにバッファの内容を上書き保存します。
    pub fn save(&mut self) -> io::Result<()> {
        if let Some(ref path) = self.path {
            let file = File::create(path)?;
            self.rope.write_to(BufWriter::new(file))?;
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
        self.is_modified = false;
        Ok(())
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

    /// 指定した行・列位置に文字を挿入します。
    pub fn insert_char(&mut self, row: usize, col: usize, c: char) {
        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert_char(char_idx, c);
        self.is_modified = true;
    }

    /// 指定した行・列位置の文字（カーソル位置の文字）を削除します。
    pub fn delete_char(&mut self, row: usize, col: usize) {
        let char_idx = self.point_to_char_idx(row, col);
        if char_idx < self.rope.len_chars() {
            self.rope.remove(char_idx..char_idx + 1);
            self.is_modified = true;
        }
    }

    /// 指定した行・列位置の直前の文字（バックスペース）を削除し、
    /// 削除後の新しい (row, col) 位置を返します。
    pub fn delete_backspace(&mut self, row: usize, col: usize) -> Option<(usize, usize)> {
        let char_idx = self.point_to_char_idx(row, col);
        if char_idx > 0 {
            self.rope.remove(char_idx - 1..char_idx);
            self.is_modified = true;
            let new_char_idx = char_idx - 1;
            let new_row = self.rope.char_to_line(new_char_idx);
            let line_start_char = self.rope.line_to_char(new_row);
            let new_col = new_char_idx - line_start_char;
            Some((new_row, new_col))
        } else {
            None
        }
    }

    /// 指定した行・列位置に改行を挿入し、新しい (row, col) 位置 `(row + 1, 0)` を返します。
    pub fn insert_newline(&mut self, row: usize, col: usize) -> (usize, usize) {
        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert_char(char_idx, '\n');
        self.is_modified = true;
        (row + 1, 0)
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
}
