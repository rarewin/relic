mod buffer;
mod completion;
mod editor;
mod syntax;

use std::env;
use std::io::{self, stdout};
use std::panic;

use crossterm::{
    event::{
        self, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use buffer::Buffer;
use editor::Editor;

/// ターミナル終了時にRawモードの無効化、代替画面の離脱、キーボード拡張フラグのリセットを保証するRAIIガード。
struct TerminalGuard {
    enhanced_keyboard: bool,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.enhanced_keyboard {
            let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

/// パニック発生時に端末の状態を正常に復元するためのカスタムパニックフックを設定します。
fn setup_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

/// アプリケーションのエントリポイント。
///
/// コマンドライン引数からファイルパスを取得してバッファを初期化し、
/// 端末をRawモードへ切り替えてメインイベントループを実行します。
fn main() -> io::Result<()> {
    setup_panic_hook();

    let args: Vec<String> = env::args().collect();
    let buffer = if let Some(path) = args.get(1) {
        Buffer::open(path)?
    } else {
        Buffer::new()
    };

    // Initialize Terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let enhanced_keyboard = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced_keyboard {
        let _ = execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        );
    }
    let _guard = TerminalGuard { enhanced_keyboard };

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut editor = Editor::new(buffer);

    // Event loop
    loop {
        terminal.draw(|f| editor.render(f))?;

        if editor.should_quit {
            break;
        }

        if let Event::Key(key) = event::read()? {
            // Only process KeyPress events (filter out KeyRelease / Repeat on Windows/newer crossterm)
            if key.kind == KeyEventKind::Press {
                editor.handle_key_event(key);
            }
        }
    }

    Ok(())
}
