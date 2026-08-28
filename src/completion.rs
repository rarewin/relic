use std::env;
use std::fs;
use std::path::PathBuf;

/// ファイルパス補完の結果を表す構造体。
pub struct CompletionResult {
    /// 補完によって確定・伸長された新しい入力文字列（変更がない場合は `None`）
    pub completed: Option<String>,
    /// マッチした候補ファイル・ディレクトリ名の一覧
    pub candidates: Vec<String>,
    /// 補完対象ディレクトリの元のプレフィックス部分（例: `src/`）
    pub raw_dir_part: String,
}

/// パス文字列中のチルダ（`~` や `~/...`）をホームディレクトリの実パスに展開します。
pub fn expand_tilde(path_str: &str) -> String {
    if path_str == "~" {
        if let Ok(home) = env::var("HOME") {
            return home;
        }
    } else if let Some(stripped) = path_str.strip_prefix("~/")
        && let Ok(home) = env::var("HOME")
    {
        return format!("{}/{}", home, stripped);
    }
    path_str.to_string()
}

/// 入力されたパス文字列に基づいてファイルシステムを探索し、補完結果を返します。
///
/// 大文字・小文字を区別せず（case-insensitive）前方一致マッチングを行います。
/// ディレクトリの場合は候補名の末尾に `/` が付加されます。
pub fn complete_path(input: &str) -> CompletionResult {
    let expanded_input = expand_tilde(input);

    let (dir_part, file_part) = match expanded_input.rfind('/') {
        Some(idx) => (&expanded_input[..=idx], &expanded_input[idx + 1..]),
        None => ("", expanded_input.as_str()),
    };

    let search_dir = if dir_part.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(dir_part)
    };

    let raw_dir_part = match input.rfind('/') {
        Some(idx) => &input[..=idx],
        None => "",
    };

    let entries = match fs::read_dir(&search_dir) {
        Ok(read_dir) => read_dir,
        Err(_) => {
            return CompletionResult {
                completed: None,
                candidates: Vec::new(),
                raw_dir_part: raw_dir_part.to_string(),
            };
        }
    };

    let mut matches = Vec::new();
    let file_part_lower = file_part.to_lowercase();
    let include_hidden = file_part.starts_with('.');

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !include_hidden && name.starts_with('.') {
            continue;
        }

        let name_lower = name.to_lowercase();
        if name_lower.starts_with(&file_part_lower) {
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if is_dir {
                matches.push(format!("{}/", name));
            } else {
                matches.push(name);
            }
        }
    }

    if matches.is_empty() {
        return CompletionResult {
            completed: None,
            candidates: Vec::new(),
            raw_dir_part: raw_dir_part.to_string(),
        };
    }

    matches.sort();

    if matches.len() == 1 {
        let single = &matches[0];
        let completed = format!("{}{}", raw_dir_part, single);
        return CompletionResult {
            completed: Some(completed),
            candidates: matches,
            raw_dir_part: raw_dir_part.to_string(),
        };
    }

    // 複数候補がある場合、最長共通プレフィックス（LCP）を求める
    let mut lcp = matches[0].clone();
    for item in &matches[1..] {
        let mut common_len = 0;
        for (c1, c2) in lcp.chars().zip(item.chars()) {
            if c1.to_lowercase().to_string() == c2.to_lowercase().to_string() {
                common_len += c1.len_utf8();
            } else {
                break;
            }
        }
        lcp.truncate(common_len);
    }

    let completed_str = format!("{}{}", raw_dir_part, lcp);
    let completed = if !lcp.is_empty() && completed_str != input {
        Some(completed_str)
    } else {
        None
    };

    CompletionResult {
        completed,
        candidates: matches,
        raw_dir_part: raw_dir_part.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_cargo() {
        let res = complete_path("car");
        assert!(res.candidates.iter().any(|c| c.starts_with("Cargo.toml")));
    }

    #[test]
    fn test_completion_src() {
        let res = complete_path("src/");
        assert!(res.candidates.iter().any(|c| c.starts_with("main.rs")));
    }
}
