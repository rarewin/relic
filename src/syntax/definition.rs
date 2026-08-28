use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    pub pattern: String,
    pub style: String,
    pub group: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub regex: Regex,
    pub style: String,
    pub group: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxConfig {
    pub name: String,
    pub file_extensions: Vec<String>,
    pub line_comment: Option<String>,
    pub rules: Vec<RuleConfig>,
}

#[derive(Debug, Clone)]
pub struct SyntaxDefinition {
    pub name: String,
    pub file_extensions: Vec<String>,
    pub line_comment: Option<String>,
    pub compiled_rules: Vec<CompiledRule>,
}

impl SyntaxDefinition {
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
