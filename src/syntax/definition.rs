use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 設定ファイル（TOML等）で定義される個別の構文ハイライトルール。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// マッチ対象の正規表現パターン
    pub pattern: String,
    /// 適用するスタイル名（`Theme` で定義されているトークン名）
    pub style: String,
    /// スタイルを適用するキャプチャグループの番号（指定しない場合は全体 `0`）
    pub group: Option<usize>,
}

/// コンパイル済みの正規表現を持つ構文ハイライトルール。
#[derive(Debug, Clone)]
pub struct CompiledRule {
    /// コンパイル済み正規表現
    pub regex: Regex,
    /// 適用するスタイル名
    pub style: String,
    /// 対象キャプチャグループ番号
    pub group: usize,
}

/// 設定ファイルから読み込まれる構文定義の生データ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxConfig {
    /// 言語またはフォーマットの名称（例: `"Rust"`, `"Markdown"`）
    pub name: String,
    /// 対象となるファイル拡張子の一覧（例: `["rs"]`, `["c", "cpp"]`）
    pub file_extensions: Vec<String>,
    /// 単一行コメントの開始プレフィックス（例: `"//"`）
    pub line_comment: Option<String>,
    /// ハイライトルールのリスト
    pub rules: Vec<RuleConfig>,
}

/// エディタ内で実際に使用されるコンパイル済みの構文定義。
#[derive(Debug, Clone)]
pub struct SyntaxDefinition {
    /// 言語またはフォーマットの名称
    pub name: String,
    /// 対象となるファイル拡張子の一覧
    pub file_extensions: Vec<String>,
    /// 単一行コメントの開始プレフィックス
    pub line_comment: Option<String>,
    /// コンパイル済みのハイライトルール一覧
    pub compiled_rules: Vec<CompiledRule>,
}

impl SyntaxDefinition {
    /// [`SyntaxConfig`] から正規表現をコンパイルし、[`SyntaxDefinition`] を構築します。
    pub fn from_config(config: SyntaxConfig) -> Result<Self, regex::Error> {
        let mut compiled_rules = Vec::new();
        for rc in config.rules {
            let regex = Regex::new(&rc.pattern)?;
            compiled_rules.push(CompiledRule {
                regex,
                style: rc.style,
                group: rc.group.unwrap_or(0),
            });
        }

        Ok(Self {
            name: config.name,
            file_extensions: config.file_extensions,
            line_comment: config.line_comment,
            compiled_rules,
        })
    }

    /// 与えられたファイルパスの拡張子がこの構文定義にマッチするか判定します。
    pub fn matches_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let ext_lower = ext.to_lowercase();
            return self
                .file_extensions
                .iter()
                .any(|fe| fe.to_lowercase() == ext_lower);
        }
        false
    }
}
