use rhai::{AST, Dynamic, Engine, Scope};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::completion::expand_tilde;

/// エディタの現在の状態スナップショット（Rhaiスクリプトからの情報取得用）。
#[derive(Debug, Default, Clone)]
pub struct EditorContext {
    /// カーソル行（0-indexed）
    pub cursor_row: usize,
    /// カーソル列（0-indexed）
    pub cursor_col: usize,
    /// 現在の行の文字列（末尾改行除く）
    pub current_line: String,
    /// バッファ全体の行数
    #[allow(dead_code)]
    pub total_lines: usize,
}

impl EditorContext {
    /// 現在行における最初の非空白文字の列インデックス（インデント位置）を計算します。
    pub fn indentation_col(&self) -> usize {
        self.current_line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count()
    }
}

/// キーバインド文字列を標準フォーマット（例: "C-a", "M-x", "C-x C-f"）に正規化します。
pub fn normalize_key_binding(raw: &str) -> String {
    let s = raw.trim();
    // \C-a -> C-a, \M-x -> M-x などの Emacs 表記のバックスラッシュ除去
    let s = s
        .replace("\\C-", "C-")
        .replace("\\M-", "M-")
        .replace("\\e", "ESC");
    // Ctrl- -> C-, Alt- -> M-, Meta- -> M-
    let s = s
        .replace("Ctrl-", "C-")
        .replace("ctrl-", "C-")
        .replace("Alt-", "M-")
        .replace("alt-", "M-")
        .replace("Meta-", "M-")
        .replace("meta-", "M-");
    // C-Space / C-space -> C-SPC
    s.replace("C-Space", "C-SPC").replace("C-space", "C-SPC")
}

/// スクリプト実行時にエディタへ要求する操作コマンドキュー。
#[derive(Debug, Default, Clone)]
pub struct ScriptCommandQueue {
    /// エコーエリアに表示するメッセージ
    pub message: Option<String>,
    /// カーソル位置に挿入するテキスト
    pub inserts: Vec<String>,
    /// カーソル移動要求 (row, col)
    pub cursor_move: Option<(usize, usize)>,
}

/// エディタの設定・拡張スクリプトを管理・実行する Rhai プラグインエンジン。
pub struct PluginEngine {
    /// Rhai スクリプト評価エンジン
    pub engine: Engine,
    /// コンパイル済み AST（読み込まれている場合）
    pub ast: Option<AST>,
    /// スクリプトからエディタへのコマンド伝達用共有キュー
    pub command_queue: Arc<Mutex<ScriptCommandQueue>>,
    /// エディタの現在状態スナップショット
    pub editor_context: Arc<Mutex<EditorContext>>,
    /// カスタムキーバインド定義マップ（正規化キー名 -> Rhai 関数名）
    pub keybindings: Arc<Mutex<HashMap<String, String>>>,
}

impl PluginEngine {
    /// 新しい [`PluginEngine`] インスタンスを初期化し、エディタ API を登録します。
    pub fn new() -> Self {
        let mut engine = Engine::new();
        let command_queue = Arc::new(Mutex::new(ScriptCommandQueue::default()));
        let editor_context = Arc::new(Mutex::new(EditorContext::default()));
        let keybindings = Arc::new(Mutex::new(HashMap::new()));

        // API: relic::version() -> String
        engine.register_fn("version", || -> String {
            env!("CARGO_PKG_VERSION").to_string()
        });

        // API: relic::set_message(msg)
        let q_clone = Arc::clone(&command_queue);
        engine.register_fn("set_message", move |msg: &str| {
            if let Ok(mut q) = q_clone.lock() {
                q.message = Some(msg.to_string());
            }
        });

        // API: relic::insert_text(text)
        let q_clone = Arc::clone(&command_queue);
        engine.register_fn("insert_text", move |text: &str| {
            if let Ok(mut q) = q_clone.lock() {
                q.inserts.push(text.to_string());
            }
        });

        // API: relic::move_cursor(row, col)
        let q_clone = Arc::clone(&command_queue);
        engine.register_fn("move_cursor", move |row: i64, col: i64| {
            if let Ok(mut q) = q_clone.lock() {
                let r = row.max(0) as usize;
                let c = col.max(0) as usize;
                q.cursor_move = Some((r, c));
            }
        });

        // API: print / log ヘルパー
        let q_clone = Arc::clone(&command_queue);
        engine.register_fn("show_version", move || {
            if let Ok(mut q) = q_clone.lock() {
                q.message = Some(format!("Relic Editor v{}", env!("CARGO_PKG_VERSION")));
            }
        });

        // --- エディタ状態取得 API ---
        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("cursor_row", move || -> i64 {
            ctx_clone
                .lock()
                .map(|ctx| ctx.cursor_row as i64)
                .unwrap_or(0)
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("cursor_col", move || -> i64 {
            ctx_clone
                .lock()
                .map(|ctx| ctx.cursor_col as i64)
                .unwrap_or(0)
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("is_bol", move || -> bool {
            ctx_clone
                .lock()
                .map(|ctx| ctx.cursor_col == 0)
                .unwrap_or(false)
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("current_line", move || -> String {
            ctx_clone
                .lock()
                .map(|ctx| ctx.current_line.clone())
                .unwrap_or_default()
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("line_len", move || -> i64 {
            ctx_clone
                .lock()
                .map(|ctx| ctx.current_line.chars().count() as i64)
                .unwrap_or(0)
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("indentation_col", move || -> i64 {
            ctx_clone
                .lock()
                .map(|ctx| ctx.indentation_col() as i64)
                .unwrap_or(0)
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("is_at_indentation", move || -> bool {
            ctx_clone
                .lock()
                .map(|ctx| ctx.cursor_col == ctx.indentation_col())
                .unwrap_or(false)
        });

        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("looking_at", move |pattern: &str| -> bool {
            let Ok(ctx) = ctx_clone.lock() else {
                return false;
            };
            if pattern == "^" {
                ctx.cursor_col == 0
            } else if pattern == "$" {
                ctx.cursor_col >= ctx.current_line.chars().count()
            } else {
                let remaining: String = ctx.current_line.chars().skip(ctx.cursor_col).collect();
                remaining.starts_with(pattern)
            }
        });

        // --- エディタ移動コマンド API ---
        let q_clone = Arc::clone(&command_queue);
        let ctx_clone = Arc::clone(&editor_context);
        let move_to_bol_fn = move || {
            let row = ctx_clone.lock().map(|c| c.cursor_row).unwrap_or(0);
            if let Ok(mut q) = q_clone.lock() {
                q.cursor_move = Some((row, 0));
            }
        };
        engine.register_fn("move_to_bol", move_to_bol_fn.clone());
        engine.register_fn("beginning_of_line", move_to_bol_fn);

        let q_clone = Arc::clone(&command_queue);
        let ctx_clone = Arc::clone(&editor_context);
        engine.register_fn("back_to_indentation", move || {
            let (row, col) = ctx_clone
                .lock()
                .map(|c| (c.cursor_row, c.indentation_col()))
                .unwrap_or((0, 0));
            if let Ok(mut q) = q_clone.lock() {
                q.cursor_move = Some((row, col));
            }
        });

        let q_clone = Arc::clone(&command_queue);
        let ctx_clone = Arc::clone(&editor_context);
        let move_to_eol_fn = move || {
            let (row, col) = ctx_clone
                .lock()
                .map(|c| (c.cursor_row, c.current_line.chars().count()))
                .unwrap_or((0, 0));
            if let Ok(mut q) = q_clone.lock() {
                q.cursor_move = Some((row, col));
            }
        };
        engine.register_fn("move_to_eol", move_to_eol_fn.clone());
        engine.register_fn("end_of_line", move_to_eol_fn);

        // --- キーバインド登録 API ---
        let kb_clone = Arc::clone(&keybindings);
        let bind_key_fn = move |key: &str, func_name: &str| {
            let norm = normalize_key_binding(key);
            if let Ok(mut map) = kb_clone.lock() {
                map.insert(norm, func_name.to_string());
            }
        };
        engine.register_fn("bind_key", bind_key_fn.clone());
        engine.register_fn("define_key", bind_key_fn);

        Self {
            engine,
            ast: None,
            command_queue,
            editor_context,
            keybindings,
        }
    }

    /// 設定スクリプトファイル（`~/.config/relic/init.rhai`）の標準パスを取得します。
    pub fn default_config_path() -> PathBuf {
        let expanded = expand_tilde("~/.config/relic/init.rhai");
        PathBuf::from(expanded)
    }

    /// エディタの最新状態を共有コンテキストへ同期します。
    pub fn update_context(&self, ctx: EditorContext) {
        if let Ok(mut c) = self.editor_context.lock() {
            *c = ctx;
        }
    }

    /// 指定されたキーシーケンスにバインドされた Rhai 関数名を検索します。
    pub fn lookup_keybinding(&self, key: &str) -> Option<String> {
        let norm = normalize_key_binding(key);
        self.keybindings.lock().ok()?.get(&norm).cloned()
    }

    /// 指定されたパスの Rhai スクリプトファイルを読み込んでコンパイル・評価します。
    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Err(format!("Config file not found: {}", path_ref.display()));
        }

        let script = fs::read_to_string(path_ref)
            .map_err(|e| format!("Failed to read script file: {}", e))?;

        let ast = self
            .engine
            .compile(&script)
            .map_err(|e| format!("Script syntax error: {}", e))?;

        let mut scope = Scope::new();
        self.engine
            .run_ast_with_scope(&mut scope, &ast)
            .map_err(|e| format!("Script runtime error: {}", e))?;

        self.ast = Some(ast);
        Ok(())
    }

    /// 定義済みのカスタム関数を名前指定で呼び出します。
    #[allow(dead_code)]
    pub fn call_fn(&self, fn_name: &str, args: impl rhai::FuncArgs) -> Result<Dynamic, String> {
        if let Some(ref ast) = self.ast {
            let mut scope = Scope::new();
            self.engine
                .call_fn(&mut scope, ast, fn_name, args)
                .map_err(|e| format!("Function call error ({}): {}", fn_name, e))
        } else {
            Err("No script AST loaded".into())
        }
    }

    /// スクリプト実行によってキューに溜まった操作コマンドを取り出し、空にします。
    pub fn take_commands(&self) -> ScriptCommandQueue {
        if let Ok(mut q) = self.command_queue.lock() {
            std::mem::take(&mut *q)
        } else {
            ScriptCommandQueue::default()
        }
    }
}

impl Default for PluginEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_version_and_message() {
        let plugin = PluginEngine::new();
        let script = r#"
            let v = version();
            set_message("Version: " + v);
        "#;
        let ast = plugin.engine.compile(script).unwrap();
        let mut scope = Scope::new();
        plugin.engine.run_ast_with_scope(&mut scope, &ast).unwrap();

        let cmds = plugin.take_commands();
        assert_eq!(
            cmds.message,
            Some(format!("Version: {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn test_plugin_insert_and_call_fn() {
        let mut plugin = PluginEngine::new();
        let script = r#"
            fn add_header() {
                insert_text("// Header\n");
            }
        "#;
        let ast = plugin.engine.compile(script).unwrap();
        plugin.ast = Some(ast);

        // Call defined function
        let _ = plugin.call_fn("add_header", ()).unwrap();
        let cmds = plugin.take_commands();
        assert_eq!(cmds.inserts, vec!["// Header\n".to_string()]);
    }

    #[test]
    fn test_plugin_keybinding_registration() {
        let plugin = PluginEngine::new();
        let script = r#"
            define_key("C-a", "lovely_bol");
            bind_key("M-x", "execute_cmd");
        "#;
        let ast = plugin.engine.compile(script).unwrap();
        let mut scope = Scope::new();
        plugin.engine.run_ast_with_scope(&mut scope, &ast).unwrap();

        assert_eq!(
            plugin.lookup_keybinding("C-a"),
            Some("lovely_bol".to_string())
        );
        assert_eq!(
            plugin.lookup_keybinding("\\C-a"),
            Some("lovely_bol".to_string())
        );
        assert_eq!(
            plugin.lookup_keybinding("M-x"),
            Some("execute_cmd".to_string())
        );
    }

    #[test]
    fn test_plugin_editor_context_and_smart_bol() {
        let mut plugin = PluginEngine::new();
        let script = r#"
            fn lovely_bol() {
                if looking_at("^") {
                    back_to_indentation();
                } else {
                    beginning_of_line();
                }
            }
        "#;
        let ast = plugin.engine.compile(script).unwrap();
        plugin.ast = Some(ast);

        // Case 1: Cursor in middle of line ("    let x = 1;", col = 8)
        plugin.update_context(EditorContext {
            cursor_row: 2,
            cursor_col: 8,
            current_line: "    let x = 1;".to_string(),
            total_lines: 5,
        });
        let _ = plugin.call_fn("lovely_bol", ()).unwrap();
        let cmds = plugin.take_commands();
        // Should move to col 0 (beginning of line)
        assert_eq!(cmds.cursor_move, Some((2, 0)));

        // Case 2: Cursor at beginning of line (col = 0)
        plugin.update_context(EditorContext {
            cursor_row: 2,
            cursor_col: 0,
            current_line: "    let x = 1;".to_string(),
            total_lines: 5,
        });
        let _ = plugin.call_fn("lovely_bol", ()).unwrap();
        let cmds = plugin.take_commands();
        // Should move to indentation (col = 4)
        assert_eq!(cmds.cursor_move, Some((2, 4)));
    }
}
