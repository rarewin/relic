use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 個別の構文トークンに対する前景色・背景色・字体装飾の設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConfig {
    /// 前景色（色名または `#RRGGBB`）
    pub fg: Option<String>,
    /// 背景色（色名または `#RRGGBB`）
    pub bg: Option<String>,
    /// 太字にするかどうか
    pub bold: Option<bool>,
    /// 斜体（イタリック）にするかどうか
    pub italic: Option<bool>,
    /// 下線を引くかどうか
    pub underline: Option<bool>,
}

impl StyleConfig {
    /// 設定内容を Ratatui の [`Style`] オブジェクトに変換します。
    pub fn to_ratatui_style(&self) -> Style {
        let mut style = Style::default();

        if let Some(ref fg_str) = self.fg
            && let Some(color) = parse_color(fg_str)
        {
            style = style.fg(color);
        }

        if let Some(ref bg_str) = self.bg
            && let Some(color) = parse_color(bg_str)
        {
            style = style.bg(color);
        }

        if self.bold.unwrap_or(false) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic.unwrap_or(false) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.underline.unwrap_or(false) {
            style = style.add_modifier(Modifier::UNDERLINED);
        }

        style
    }
}

/// 色名文字列（例: `"Red"`, `"Cyan"`, `"DarkGray"`）または16進数カラーコード（例: `"#ff79c6"`）を
/// Ratatui の [`Color`] にパースします。
pub fn parse_color(s: &str) -> Option<Color> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" | "purple" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" | "dark_gray" => Some(Color::DarkGray),
        "lightred" | "light_red" => Some(Color::LightRed),
        "lightgreen" | "light_green" => Some(Color::LightGreen),
        "lightyellow" | "light_yellow" => Some(Color::LightYellow),
        "lightblue" | "light_blue" => Some(Color::LightBlue),
        "lightmagenta" | "light_magenta" => Some(Color::LightMagenta),
        "lightcyan" | "light_cyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => {
            // Hex color: #RRGGBB
            if let Some(hex) = lower.strip_prefix('#')
                && hex.len() == 6
                && let Ok(r) = u8::from_str_radix(&hex[0..2], 16)
                && let Ok(g) = u8::from_str_radix(&hex[2..4], 16)
                && let Ok(b) = u8::from_str_radix(&hex[4..6], 16)
            {
                return Some(Color::Rgb(r, g, b));
            }
            None
        }
    }
}

/// エディタ全体のカラースキームと構文スタイルを保持するテーマ構造体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    /// テーマの名称
    pub name: String,
    /// トークン名（`"keyword"`, `"string"`, `"comment"` 等）とスタイルのマップ
    pub styles: HashMap<String, StyleConfig>,
}

impl Theme {
    /// デフォルトのダークテーマ（One Dark / VS Codeライクな配色）を生成します。
    pub fn default_dark() -> Self {
        let mut styles = HashMap::new();

        let add_style = |map: &mut HashMap<String, StyleConfig>,
                         key: &str,
                         fg: Option<&str>,
                         bold: bool,
                         italic: bool,
                         underline: bool| {
            map.insert(
                key.to_string(),
                StyleConfig {
                    fg: fg.map(|s| s.to_string()),
                    bg: None,
                    bold: if bold { Some(true) } else { None },
                    italic: if italic { Some(true) } else { None },
                    underline: if underline { Some(true) } else { None },
                },
            );
        };

        add_style(&mut styles, "keyword", Some("Magenta"), true, false, false);
        add_style(&mut styles, "type", Some("Cyan"), false, false, false);
        add_style(&mut styles, "function", Some("Blue"), true, false, false);
        add_style(&mut styles, "string", Some("Green"), false, false, false);
        add_style(
            &mut styles,
            "number",
            Some("LightYellow"),
            false,
            false,
            false,
        );
        add_style(&mut styles, "comment", Some("DarkGray"), false, true, false);
        add_style(&mut styles, "macro", Some("Cyan"), true, false, false);
        add_style(
            &mut styles,
            "attribute",
            Some("LightYellow"),
            false,
            false,
            false,
        );
        add_style(&mut styles, "header", Some("Cyan"), true, false, false);
        add_style(&mut styles, "bold", None, true, false, false);
        add_style(&mut styles, "italic", None, false, true, false);
        add_style(&mut styles, "link", Some("Blue"), false, false, true);
        add_style(&mut styles, "error", Some("Red"), true, false, true);
        add_style(&mut styles, "warning", Some("Yellow"), true, false, false);

        Theme {
            name: "default-dark".to_string(),
            styles,
        }
    }

    /// 指定されたトークン名（例: `"keyword"`）に対応する [`Style`] を取得します。
    /// 未定義の場合はデフォルトの空スタイルを返します。
    pub fn get_style(&self, token: &str) -> Style {
        self.styles
            .get(token)
            .map(|sc| sc.to_ratatui_style())
            .unwrap_or_default()
    }
}
