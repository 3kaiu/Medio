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
    if let Err(err) = run_inner(path, config) {
        eprintln!("TUI error: {err}");
    }
}

fn run_inner(path: &str, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let app_path = if path.is_empty() {
        ".".into()
    } else {
        path.to_string()
    };
    let mut app = App::new(config.clone(), app_path);
    let mut terminal = TerminalSession::enter()?;

    // Auto-scan on start
    app.scan();

    // Main loop
    loop {
        terminal
            .terminal
            .draw(|f| ui::draw(f, &app))
            .map_err(|err| format!("Failed to draw TUI frame: {err}"))?;
        match event::handle_events(&mut app) {
            Ok(running) => {
                if !running {
                    break;
                }
            }
            Err(err) => return Err(format!("Failed to read terminal events: {err}").into()),
        }
    }

    Ok(())
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode().map_err(|err| format!("Failed to enable raw mode: {err}"))?;

        let mut stdout = io::stdout();
        if let Err(err) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(format!("Failed to initialize terminal screen: {err}").into());
        }

        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(err) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
                let _ = disable_raw_mode();
                Err(format!("Failed to create terminal backend: {err}").into())
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}
