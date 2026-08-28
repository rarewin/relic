pub mod definition;
pub mod theme;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use ratatui::style::Style;
use ratatui::text::Span;

use definition::{RuleConfig, SyntaxConfig, SyntaxDefinition};
use theme::Theme;

/// 利用可能なすべての構文定義とテーマを管理するレジストリ。
pub struct SyntaxRegistry {
    /// 登録済みの構文定義一覧
    pub definitions: Vec<SyntaxDefinition>,
    /// 現在適用されているテーマ
    pub theme: Theme,
}

impl Default for SyntaxRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxRegistry {
    /// 組み込み構文定義および `~/.config/relic/syntax/` からのカスタム定義をロードして初期化します。
    pub fn new() -> Self {
        let mut reg = Self {
            definitions: Vec::new(),
            theme: Theme::default_dark(),
        };

        // 1. 内蔵のシンタックス定義を登録
        reg.register_builtin_syntaxes();

        // 2. 外部設定ディレクトリ（~/.config/relic/syntax/）からカスタム定義を読み込む
        reg.load_custom_syntaxes();

        reg
    }

    /// ファイルパスの拡張子に基づいて、一致する [`SyntaxDefinition`] を探索します。
    pub fn find_syntax_for_file(&self, path: &Path) -> Option<&SyntaxDefinition> {
        self.definitions.iter().find(|sd| sd.matches_file(path))
    }

    fn register_builtin_syntaxes(&mut self) {
        // Rust
        if let Ok(rust_syntax) = SyntaxDefinition::from_config(SyntaxConfig {
            name: "Rust".to_string(),
            file_extensions: vec!["rs".to_string()],
            line_comment: Some("//".to_string()),
            rules: vec![
                // Attributes
                RuleConfig {
                    pattern: r"#\[.*?\]".to_string(),
                    style: "attribute".to_string(),
                    group: None,
                },
                // Raw Strings & Regular Strings
                RuleConfig {
                    pattern: r##"r#".*?"#|"(?:[^"\\]|\\.)*""##.to_string(),
                    style: "string".to_string(),
                    group: None,
                },
                // Chars
                RuleConfig {
                    pattern: r"'(?:[^'\\]|\\.)'".to_string(),
                    style: "string".to_string(),
                    group: None,
                },
                // Numbers
                RuleConfig {
                    pattern: r"\b(?:0x[0-9a-fA-F_]+|0b[01_]+|0o[0-7_]+|[0-9][0-9_]*(?:\.[0-9_]+)?(?:[eE][+-]?[0-9_]+)?(?:f32|f64|u\d+|i\d+|usize|isize)?)\b".to_string(),
                    style: "number".to_string(),
                    group: None,
                },
                // Macros
                RuleConfig {
                    pattern: r"\b[a-zA-Z_][a-zA-Z0-9_]*!".to_string(),
                    style: "macro".to_string(),
                    group: None,
                },
                // Keywords
                RuleConfig {
                    pattern: r"\b(?:fn|let|mut|if|else|match|while|for|in|loop|return|struct|enum|impl|trait|type|pub|use|mod|crate|self|Self|super|where|async|await|const|static|move|ref|unsafe|extern|as|break|continue|dyn|yield)\b".to_string(),
                    style: "keyword".to_string(),
                    group: None,
                },
                // Built-in types
                RuleConfig {
                    pattern: r"\b(?:i8|i16|i32|i64|i128|u8|u16|u32|u64|u128|isize|usize|f32|f64|bool|char|str|String|Vec|Option|Some|None|Result|Ok|Err|Box)\b".to_string(),
                    style: "type".to_string(),
                    group: None,
                },
                // Functions
                RuleConfig {
                    pattern: r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(".to_string(),
                    style: "function".to_string(),
                    group: Some(1),
                },
            ],
        }) {
            self.definitions.push(rust_syntax);
        }

        // C / C++
        if let Ok(c_syntax) = SyntaxDefinition::from_config(SyntaxConfig {
            name: "C/C++".to_string(),
            file_extensions: vec![
                "c".to_string(),
                "cpp".to_string(),
                "cc".to_string(),
                "cxx".to_string(),
                "h".to_string(),
                "hpp".to_string(),
                "hxx".to_string(),
                "c++".to_string(),
                "h++".to_string(),
            ],
            line_comment: Some("//".to_string()),
            rules: vec![
                // Preprocessor
                RuleConfig {
                    pattern: r"^\s*#[a-zA-Z_]+".to_string(),
                    style: "macro".to_string(),
                    group: None,
                },
                // Strings
                RuleConfig {
                    pattern: r#""(?:[^"\\]|\\.)*""#.to_string(),
                    style: "string".to_string(),
                    group: None,
                },
                // Chars
                RuleConfig {
                    pattern: r"'(?:[^'\\]|\\.)'".to_string(),
                    style: "string".to_string(),
                    group: None,
                },
                // Numbers
                RuleConfig {
                    pattern: r"\b(?:0x[0-9a-fA-F]+|\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)\b".to_string(),
                    style: "number".to_string(),
                    group: None,
                },
                // Keywords
                RuleConfig {
                    pattern: r"\b(?:if|else|while|for|do|return|switch|case|break|continue|default|goto|sizeof|typedef|struct|union|enum|class|public|private|protected|virtual|override|constexpr|const|volatile|static|extern|inline|template|typename|namespace|using|new|delete|try|catch|throw|auto|decltype|explicit|friend|operator|nullptr)\b".to_string(),
                    style: "keyword".to_string(),
                    group: None,
                },
                // Built-in types
                RuleConfig {
                    pattern: r"\b(?:void|int|char|short|long|float|double|signed|unsigned|bool|size_t|int8_t|int16_t|int32_t|int64_t|uint8_t|uint16_t|uint32_t|uint64_t|std::[a-zA-Z0-9_]+)\b".to_string(),
                    style: "type".to_string(),
                    group: None,
                },
                // Functions
                RuleConfig {
                    pattern: r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(".to_string(),
                    style: "function".to_string(),
                    group: Some(1),
                },
            ],
        }) {
            self.definitions.push(c_syntax);
        }

        // Markdown
        if let Ok(md_syntax) = SyntaxDefinition::from_config(SyntaxConfig {
            name: "Markdown".to_string(),
            file_extensions: vec!["md".to_string(), "markdown".to_string()],
            line_comment: None,
            rules: vec![
                // Headers
                RuleConfig {
                    pattern: r"^#{1,6}\s+.*$".to_string(),
                    style: "header".to_string(),
                    group: None,
                },
                // Code block fence
                RuleConfig {
                    pattern: r"^```.*$".to_string(),
                    style: "keyword".to_string(),
                    group: None,
                },
                // Inline Code
                RuleConfig {
                    pattern: r"`[^`]+`".to_string(),
                    style: "string".to_string(),
                    group: None,
                },
                // Blockquote
                RuleConfig {
                    pattern: r"^\s*>.*$".to_string(),
                    style: "comment".to_string(),
                    group: None,
                },
                // Lists
                RuleConfig {
                    pattern: r"^\s*(?:[-*+]|\d+\.)\s".to_string(),
                    style: "keyword".to_string(),
                    group: None,
                },
                // Links
                RuleConfig {
                    pattern: r"\[([^\]]+)\]\(([^)]+)\)".to_string(),
                    style: "link".to_string(),
                    group: None,
                },
                // Bold
                RuleConfig {
                    pattern: r"\*\*[^*]+\*\*|__[^_]+__".to_string(),
                    style: "bold".to_string(),
                    group: None,
                },
                // Italic
                RuleConfig {
                    pattern: r"\*[^*]+\*|_[^_]+_".to_string(),
                    style: "italic".to_string(),
                    group: None,
                },
            ],
        }) {
            self.definitions.push(md_syntax);
        }
    }

    fn load_custom_syntaxes(&mut self) {
        if let Ok(home) = env::var("HOME") {
            let syntax_dir = PathBuf::from(home).join(".config/relic/syntax");
            if let Ok(entries) = fs::read_dir(syntax_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("toml")
                        && let Ok(content) = fs::read_to_string(&path)
                        && let Ok(config) = toml::from_str::<SyntaxConfig>(&content)
                        && let Ok(syntax_def) = SyntaxDefinition::from_config(config)
                    {
                        // ユーザー定義を優先（先頭に追加）
                        self.definitions.insert(0, syntax_def);
                    }
                }
            }
        }
    }
}

/// 1行の文字列に対して構文解析・スタイル割り当てを行い、Ratatui の [`Span`] リストとして返します。
///
/// 構文定義が指定されていない場合はプレーンテキストの単一 Span を返します。
pub fn highlight_line<'a>(
    line: &'a str,
    theme: &Theme,
    syntax: Option<&SyntaxDefinition>,
) -> Vec<Span<'a>> {
    if line.is_empty() {
        return vec![Span::raw("")];
    }

    let Some(syntax) = syntax else {
        return vec![Span::raw(line)];
    };

    let len = line.len();
    let mut styles: Vec<Option<Style>> = vec![None; len];

    // 1. 各正規表現ルールをマッチさせてスタイルを割り当てる
    for rule in &syntax.compiled_rules {
        let style = theme.get_style(&rule.style);
        for caps in rule.regex.captures_iter(line) {
            if let Some(mat) = caps.get(rule.group) {
                let start = mat.start().min(len);
                let end = mat.end().min(len);
                for slot in &mut styles[start..end] {
                    *slot = Some(style);
                }
            }
        }
    }

    // 2. 行コメントが存在する場合は最優先で上書き
    if let Some(ref comment_prefix) = syntax.line_comment
        && let Some(idx) = line.find(comment_prefix)
    {
        let comment_style = theme.get_style("comment");
        for slot in &mut styles[idx..len] {
            *slot = Some(comment_style);
        }
    }

    // 3. スタイル配列をもとに連続区間を Span にまとめる
    let mut spans = Vec::new();
    let mut current_start = 0;
    let mut current_style = styles[0];

    for i in 1..len {
        if styles[i] != current_style {
            let chunk = &line[current_start..i];
            spans.push(match current_style {
                Some(style) => Span::styled(chunk, style),
                None => Span::raw(chunk),
            });
            current_start = i;
            current_style = styles[i];
        }
    }

    let chunk = &line[current_start..len];
    spans.push(match current_style {
        Some(style) => Span::styled(chunk, style),
        None => Span::raw(chunk),
    });

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_highlight() {
        let registry = SyntaxRegistry::new();
        let rust_def = registry
            .find_syntax_for_file(Path::new("main.rs"))
            .expect("Rust syntax should be found");

        let line = "fn main() { let x = 42; // comment";
        let spans = highlight_line(line, &registry.theme, Some(rust_def));

        assert!(spans.len() > 1);
    }

    #[test]
    fn test_cpp_highlight() {
        let registry = SyntaxRegistry::new();
        let c_def = registry
            .find_syntax_for_file(Path::new("test.cpp"))
            .expect("C++ syntax should be found");

        let line = "#include <iostream>\nint main() { return 0; }";
        let spans = highlight_line(line, &registry.theme, Some(c_def));
        assert!(spans.len() > 1);
    }

    #[test]
    fn test_markdown_highlight() {
        let registry = SyntaxRegistry::new();
        let md_def = registry
            .find_syntax_for_file(Path::new("README.md"))
            .expect("Markdown syntax should be found");

        let line = "# Hello **World**";
        let spans = highlight_line(line, &registry.theme, Some(md_def));
        assert!(spans.len() > 1);
    }
}
