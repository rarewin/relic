use std::env;
use std::fs;
use std::path::PathBuf;

pub struct CompletionResult {
    pub completed: Option<String>,
    pub candidates: Vec<String>,
}

fn expand_tilde(path_str: &str) -> String {
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

    let entries = match fs::read_dir(&search_dir) {
        Ok(read_dir) => read_dir,
        Err(_) => {
            return CompletionResult {
                completed: None,
                candidates: Vec::new(),
            };
        }
    };

    let mut matches = Vec::new();
    let include_hidden = file_part.starts_with('.');

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !include_hidden && name.starts_with('.') {
            continue;
        }

        if name.starts_with(file_part) {
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
        };
    }

    matches.sort();

    // ユーザーが元の入力で使っていた dir_part_for_display を求める
    let raw_dir_part = match input.rfind('/') {
        Some(idx) => &input[..=idx],
        None => "",
    };

    if matches.len() == 1 {
        let single = &matches[0];
        let completed = format!("{}{}", raw_dir_part, single);
        return CompletionResult {
            completed: Some(completed),
            candidates: matches,
        };
    }

    // 複数候補がある場合、最長共通プレフィックス（LCP）を求める
    let mut lcp = matches[0].clone();
    for item in &matches[1..] {
        let mut common_len = 0;
        for (c1, c2) in lcp.chars().zip(item.chars()) {
            if c1 == c2 {
                common_len += c1.len_utf8();
            } else {
                break;
            }
        }
        lcp.truncate(common_len);
    }

    let completed_str = format!("{}{}", raw_dir_part, lcp);
    let completed = if completed_str != input {
        Some(completed_str)
    } else {
        None
    };

    CompletionResult {
        completed,
        candidates: matches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_cargo() {
        let res = complete_path("Car");
        assert!(res.candidates.iter().any(|c| c.starts_with("Cargo.toml")));
    }
}
