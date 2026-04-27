use crate::tui::app::{App, Mode, Tab};
use crossterm::event::{self, Event, KeyCode, KeyEvent};

pub fn handle_events(app: &mut App) -> std::io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            match app.mode {
                Mode::Normal => handle_normal(app, key),
                Mode::Search => handle_search(app, key),
                Mode::Confirm => handle_confirm(app, key),
            }
        }
    }
    Ok(!app.should_quit)
}

fn handle_normal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('s') => app.scan(),
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.search_query.clear();
            app.status_msg = "Search: (Esc to cancel)".into();
        }
        KeyCode::Tab => app.next_tab(),
        KeyCode::BackTab => app.prev_tab(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
        KeyCode::Home | KeyCode::Char('g') => app.select_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_last(),
        KeyCode::PageDown => app.page_down(20),
        KeyCode::PageUp => app.page_up(20),
        KeyCode::Char('1') => { app.tab = Tab::Scan; app.selected = 0; app.scroll_offset = 0; }
        KeyCode::Char('2') => { app.tab = Tab::Dedup; app.selected = 0; app.scroll_offset = 0; }
        KeyCode::Char('3') => { app.tab = Tab::Rename; app.selected = 0; app.scroll_offset = 0; }
        KeyCode::Char('4') => { app.tab = Tab::Organize; app.selected = 0; app.scroll_offset = 0; }
        KeyCode::Enter => {
            // Show details of selected item
            if let Some((_, item)) = app.filtered_items().get(app.selected) {
                app.status_msg = format!("{:?}: {} ({})", item.media_type,
                    item.path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default(),
                    format_size(item.file_size));
            }
        }
        _ => {}
    }
}

fn handle_search(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.search_query.clear();
            app.status_msg = "Search cancelled".into();
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            let count = app.filtered_items().len();
            app.status_msg = format!("Found {count} results");
        }
        KeyCode::Backspace => {
            app.search_query.pop();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            app.mode = Mode::Normal;
            app.status_msg = "Confirmed!".into();
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.status_msg = "Cancelled".into();
        }
        _ => {}
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB { format!("{:.1} GiB", bytes as f64 / GB as f64) }
    else if bytes >= MB { format!("{:.1} MiB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{:.1} KiB", bytes as f64 / KB as f64) }
    else { format!("{bytes} B") }
}
