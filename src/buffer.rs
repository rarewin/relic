use ropey::Rope;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

pub struct Buffer {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub is_modified: bool,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            is_modified: false,
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
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

    pub fn len_lines(&self) -> usize {
        let count = self.rope.len_lines();
        if count == 0 { 1 } else { count }
    }

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

    pub fn insert_char(&mut self, row: usize, col: usize, c: char) {
        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert_char(char_idx, c);
        self.is_modified = true;
    }

    pub fn delete_char(&mut self, row: usize, col: usize) {
        let char_idx = self.point_to_char_idx(row, col);
        if char_idx < self.rope.len_chars() {
            self.rope.remove(char_idx..char_idx + 1);
            self.is_modified = true;
        }
    }

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

    pub fn insert_newline(&mut self, row: usize, col: usize) -> (usize, usize) {
        let char_idx = self.point_to_char_idx(row, col);
        self.rope.insert_char(char_idx, '\n');
        self.is_modified = true;
        (row + 1, 0)
    }

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
