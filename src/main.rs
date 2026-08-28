mod buffer;
mod completion;
mod editor;
mod syntax;

use std::env;
use std::io::{self, stdout};
use std::panic;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use buffer::Buffer;
use editor::Editor;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn setup_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}

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
    let _guard = TerminalGuard;

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
