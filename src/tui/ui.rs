use crate::models::media::MediaItem;
use crate::tui::app::{App, Mode, Tab, ViewMode};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, HighlightSpacing, Paragraph, Row, Table, Tabs, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::new(
        Direction::Vertical,
        [
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ],
    )
    .split(f.area());

    draw_tabs(f, app, chunks[0]);
    draw_content(f, app, chunks[1]);
    draw_detail(f, app, chunks[2]);
    draw_status(f, app, chunks[3]);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = vec![
        Line::from(" 1.Scan "),
        Line::from(" 2.Dedup "),
        Line::from(" 3.Rename "),
        Line::from(" 4.Organize "),
    ];
    let active = match app.tab {
        Tab::Scan => 0,
        Tab::Dedup => 1,
        Tab::Rename => 2,
        Tab::Organize => 3,
    };
    let tabs = Tabs::new(titles)
        .block(Block::bordered().title(" medio "))
        .select(active)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    // Tree view for Scan tab when view_mode is Tree
    if app.tab == Tab::Scan && app.view_mode == ViewMode::Tree {
        let filtered = app.filtered_items();
        let items: Vec<&MediaItem> = filtered.iter().map(|(_, item)| *item).collect();
        crate::tui::tree_view::draw_tree(f, &items, app.selected, area, " Files (Tree) ");
        return;
    }

    let (rows, header, widths, title) = match app.tab {
        Tab::Scan => {
            let filtered = app.filtered_items();
            let rows: Vec<Row> = filtered
                .iter()
                .map(|(_, item)| {
                    let name = item
                        .path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let mtype = format!("{:?}", item.media_type);
                    let title = item
                        .parsed
                        .as_ref()
                        .map(|p| p.raw_title.clone())
                        .unwrap_or_default();
                    let scraped = item
                        .scraped
                        .as_ref()
                        .map(|s| s.title.clone())
                        .unwrap_or_else(|| "—".into());
                    let size = super::format_size(item.file_size);
                    let score = item
                        .quality
                        .as_ref()
                        .map(|q| format!("{:.0}", q.quality_score))
                        .unwrap_or_default();
                    Row::new(vec![
                        Cell::from(super::truncate_str(&name, 30)),
                        Cell::from(mtype),
                        Cell::from(super::truncate_str(&title, 20)),
                        Cell::from(super::truncate_str(&scraped, 20)),
                        Cell::from(size),
                        Cell::from(score),
                    ])
                })
                .collect();
            (
                rows,
                Row::new(vec!["File", "Type", "Title", "Scraped", "Size", "Score"]),
                vec![
                    Constraint::Length(31),
                    Constraint::Length(8),
                    Constraint::Length(21),
                    Constraint::Length(21),
                    Constraint::Length(10),
                    Constraint::Length(5),
                ],
                format!(" {} files ", filtered.len()),
            )
        }
        Tab::Dedup => {
            let filtered = app.filtered_dedup_groups();
            let rows: Vec<Row> = filtered
                .iter()
                .map(|(_, group)| {
                    let keep = group.items.iter().filter(|it| it.is_keep).count();
                    let remove = group.items.iter().filter(|it| !it.is_keep).count();
                    let best = group
                        .items
                        .iter()
                        .map(|it| it.quality_score)
                        .fold(0.0, f64::max);
                    Row::new(vec![
                        Cell::from(super::truncate_str(&group.content_id, 34)),
                        Cell::from(group.items.len().to_string()),
                        Cell::from(keep.to_string()),
                        Cell::from(remove.to_string()),
                        Cell::from(format!("{best:.1}")),
                    ])
                })
                .collect();
            (
                rows,
                Row::new(vec!["Group", "Items", "Keep", "Remove", "Best"]),
                vec![
                    Constraint::Length(35),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(8),
                ],
                format!(" {} groups ", filtered.len()),
            )
        }
        Tab::Rename => {
            let filtered = app.filtered_rename_plans();
            let rows: Vec<Row> = filtered
                .iter()
                .map(|(_, plan)| {
                    Row::new(vec![
                        Cell::from(super::truncate_str(
                            &plan
                                .old_path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            28,
                        )),
                        Cell::from(super::truncate_str(
                            &plan
                                .new_path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            28,
                        )),
                        Cell::from(plan.subtitle_plans.len().to_string()),
                    ])
                })
                .collect();
            (
                rows,
                Row::new(vec!["Old", "New", "Subs"]),
                vec![
                    Constraint::Length(29),
                    Constraint::Length(29),
                    Constraint::Length(6),
                ],
                format!(" {} rename plans ", filtered.len()),
            )
        }
        Tab::Organize => {
            let filtered = app.filtered_organize_plans();
            let rows: Vec<Row> = filtered
                .iter()
                .map(|(_, plan)| {
                    Row::new(vec![
                        Cell::from(format!("{:?}", plan.action)),
                        Cell::from(super::truncate_str(
                            &plan
                                .source
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            24,
                        )),
                        Cell::from(super::truncate_str(
                            &plan
                                .target
                                .parent()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default(),
                            28,
                        )),
                        Cell::from(if plan.nfo_content.is_some() {
                            "nfo"
                        } else {
                            "—"
                        }),
                        Cell::from(if plan.image_urls.is_empty() {
                            "0".into()
                        } else {
                            plan.image_urls.len().to_string()
                        }),
                    ])
                })
                .collect();
            (
                rows,
                Row::new(vec!["Action", "Source", "Target Dir", "NFO", "Img"]),
                vec![
                    Constraint::Length(10),
                    Constraint::Length(25),
                    Constraint::Length(29),
                    Constraint::Length(5),
                    Constraint::Length(5),
                ],
                format!(" {} organize plans ", filtered.len()),
            )
        }
    };

    let header = header.style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
            .bg(Color::DarkGray),
    );

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::bordered().title(title))
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(0, 80, 80))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_symbol(">> ");

    let mut state = ratatui::widgets::TableState::new();
    if app.selected < app.current_len() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let detail = match app.tab {
        Tab::Scan => {
            let filtered = app.filtered_items();
            if app.selected < filtered.len() {
                let (_, item) = &filtered[app.selected];
                let mut lines = Vec::new();
                lines.push(Line::from(format!("Path: {}", item.path.display())));
                if let Some(p) = &item.parsed {
                    let mut s = format!("Title: {}", p.raw_title);
                    if let Some(y) = p.year {
                        s.push_str(&format!(" Year:{y}"));
                    }
                    if let Some(s2) = p.season {
                        s.push_str(&format!(" S{s2:02}"));
                    }
                    if let Some(e) = p.episode {
                        s.push_str(&format!("E{e:02}"));
                    }
                    lines.push(Line::from(s));
                }
                if let Some(q) = &item.quality {
                    lines.push(Line::from(format!(
                        "Quality: {} {}x{} score={:.1}",
                        q.resolution_label,
                        q.width.unwrap_or(0),
                        q.height.unwrap_or(0),
                        q.quality_score
                    )));
                }
                if let Some(s) = &item.scraped {
                    lines.push(Line::from(format!("Scraped: {:?} {}", s.source, s.title)));
                }
                lines
            } else {
                vec![Line::from("No item selected")]
            }
        }
        Tab::Dedup => {
            let filtered = app.filtered_dedup_groups();
            if app.selected < filtered.len() {
                let (_, group) = &filtered[app.selected];
                let mut lines = vec![Line::from(format!("Group: {}", group.content_id))];
                for entry in &group.items {
                    let item = &app.items[entry.index];
                    lines.push(Line::from(format!(
                        "{} {} score={:.1}",
                        if entry.is_keep { "KEEP" } else { "DROP" },
                        item.path
                            .file_name()
                            .map(|f| f.to_string_lossy())
                            .unwrap_or_default(),
                        entry.quality_score
                    )));
                }
                lines
            } else {
                vec![Line::from("No group selected")]
            }
        }
        Tab::Rename => {
            let filtered = app.filtered_rename_plans();
            if app.selected < filtered.len() {
                let (_, plan) = &filtered[app.selected];
                let mut lines = vec![
                    Line::from(format!("Old: {}", plan.old_path.display())),
                    Line::from(format!("New: {}", plan.new_path.display())),
                ];
                for sub in &plan.subtitle_plans {
                    lines.push(Line::from(format!(
                        "Sub: {} -> {}",
                        sub.old_path.display(),
                        sub.new_path.display()
                    )));
                }
                lines
            } else {
                vec![Line::from("No plan selected")]
            }
        }
        Tab::Organize => {
            let filtered = app.filtered_organize_plans();
            if app.selected < filtered.len() {
                let (_, plan) = &filtered[app.selected];
                vec![
                    Line::from(format!("Action: {:?}", plan.action)),
                    Line::from(format!("Source: {}", plan.source.display())),
                    Line::from(format!("Target: {}", plan.target.display())),
                    Line::from(format!(
                        "NFO: {}",
                        if plan.nfo_content.is_some() {
                            "yes"
                        } else {
                            "no"
                        }
                    )),
                    Line::from(format!("Images: {}", plan.image_urls.len())),
                ]
            } else {
                vec![Line::from("No plan selected")]
            }
        }
    };
    f.render_widget(
        Paragraph::new(detail)
            .block(Block::bordered().title(" Detail "))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let mode_str = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Search => "SEARCH",
        Mode::Confirm => "CONFIRM",
    };
    let mode_color = match app.mode {
        Mode::Normal => Color::Green,
        Mode::Search => Color::Yellow,
        Mode::Confirm => Color::Red,
    };
    let status = Line::from(vec![
        Span::styled(
            format!(" [{mode_str}] "),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(&app.status_msg),
        if !app.search_query.is_empty() {
            Span::styled(
                format!(" /{}", app.search_query),
                Style::default().fg(Color::Yellow),
            )
        } else {
            Span::raw("")
        },
    ]);
    f.render_widget(Paragraph::new(status), area);
}
