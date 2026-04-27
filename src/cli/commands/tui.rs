use crate::core::config::AppConfig;
use crate::tui::app::App;
use crate::tui::event;
use crate::tui::ui;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

pub fn run(path: &str, config: &AppConfig) {
    let app_path = if path.is_empty() {
        ".".into()
    } else {
        path.to_string()
    };
    let mut app = App::new(config.clone(), app_path);

    // Setup terminal
    enable_raw_mode().unwrap_or_else(|e| {
        eprintln!("Failed to enable raw mode: {e}");
        std::process::exit(1);
    });
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    // Auto-scan on start
    app.scan();

    // Main loop
    loop {
        terminal.draw(|f| ui::draw(f, &app)).unwrap();
        match event::handle_events(&mut app) {
            Ok(running) => {
                if !running {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Restore terminal
    disable_raw_mode().unwrap();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .unwrap();
    terminal.show_cursor().unwrap();
}
