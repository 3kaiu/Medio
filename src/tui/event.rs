use crate::tui::app::{App, Mode, Tab};
use crossterm::event::{self, Event, KeyCode, KeyEvent};

pub fn handle_events(app: &mut App) -> std::io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(100))?
        && let Event::Key(key) = event::read()?
    {
        match app.mode {
            Mode::Normal => handle_normal(app, key),
            Mode::Search => handle_search(app, key),
            Mode::Confirm => handle_confirm(app, key),
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
        KeyCode::Char('1') => {
            app.tab = Tab::Scan;
            app.selected = 0;
            app.scroll_offset = 0;
        }
        KeyCode::Char('2') => {
            app.tab = Tab::Dedup;
            app.selected = 0;
            app.scroll_offset = 0;
        }
        KeyCode::Char('3') => {
            app.tab = Tab::Rename;
            app.selected = 0;
            app.scroll_offset = 0;
        }
        KeyCode::Char('4') => {
            app.tab = Tab::Organize;
            app.selected = 0;
            app.scroll_offset = 0;
        }
        KeyCode::Char('v') => {
            if app.tab == Tab::Scan {
                app.toggle_view();
            }
        }
        KeyCode::Char('x') => match app.tab {
            Tab::Rename => app.request_rename_execute(),
            Tab::Dedup => app.request_dedup_execute(),
            Tab::Organize => app.request_organize_execute(),
            _ => {}
        },
        KeyCode::Enter => {
            app.status_msg = match app.tab {
                Tab::Scan => app.filtered_items().get(app.selected).map(|(_, item)| {
                    format!(
                        "{:?}: {} ({})",
                        item.media_type,
                        item.path
                            .file_name()
                            .map(|f| f.to_string_lossy())
                            .unwrap_or_default(),
                        super::format_size(item.file_size)
                    )
                }),
                Tab::Dedup => app
                    .filtered_dedup_groups()
                    .get(app.selected)
                    .map(|(_, group)| {
                        format!(
                            "Duplicate group {} ({} items)",
                            group.content_id,
                            group.items.len()
                        )
                    }),
                Tab::Rename => app
                    .filtered_rename_plans()
                    .get(app.selected)
                    .map(|(_, plan)| {
                        format!(
                            "Rename {} -> {}",
                            plan.old_path
                                .file_name()
                                .map(|f| f.to_string_lossy())
                                .unwrap_or_default(),
                            plan.new_path
                                .file_name()
                                .map(|f| f.to_string_lossy())
                                .unwrap_or_default()
                        )
                    }),
                Tab::Organize => {
                    app.filtered_organize_plans()
                        .get(app.selected)
                        .map(|(_, plan)| {
                            format!(
                                "Organize {} -> {}",
                                plan.source
                                    .file_name()
                                    .map(|f| f.to_string_lossy())
                                    .unwrap_or_default(),
                                plan.target.display()
                            )
                        })
                }
            }
            .unwrap_or_else(|| "No item selected".into());
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
            let count = app.current_len();
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
            app.confirm_pending_action();
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.cancel_pending_action();
        }
        _ => {}
    }
}
