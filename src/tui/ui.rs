use crate::tui::app::{App, Mode, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Cell, HighlightSpacing, Paragraph, Row, Table, Tabs, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::new(Direction::Vertical, [
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(1),
    ]).split(f.area());

    draw_tabs(f, app, chunks[0]);
    draw_content(f, app, chunks[1]);
    draw_detail(f, app, chunks[2]);
    draw_status(f, app, chunks[3]);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = vec![
        Line::from(" 1.Scan "), Line::from(" 2.Dedup "),
        Line::from(" 3.Rename "), Line::from(" 4.Organize "),
    ];
    let active = match app.tab {
        Tab::Scan => 0, Tab::Dedup => 1, Tab::Rename => 2, Tab::Organize => 3,
    };
    let tabs = Tabs::new(titles)
        .block(Block::bordered().title(" medio "))
        .select(active)
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    let filtered = app.filtered_items();
    let rows: Vec<Row> = filtered.iter().map(|(_, item)| {
        let name = item.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let mtype = format!("{:?}", item.media_type);
        let title = item.parsed.as_ref().map(|p| p.raw_title.clone()).unwrap_or_default();
        let scraped = item.scraped.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| "—".into());
        let size = format_size(item.file_size);
        let score = item.quality.as_ref().map(|q| format!("{:.0}", q.quality_score)).unwrap_or_default();
        Row::new(vec![
            Cell::from(truncate_str(&name, 30)),
            Cell::from(mtype),
            Cell::from(truncate_str(&title, 20)),
            Cell::from(truncate_str(&scraped, 20)),
            Cell::from(size),
            Cell::from(score),
        ])
    }).collect();

    let header = Row::new(vec!["File", "Type", "Title", "Scraped", "Size", "Score"])
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).bg(Color::DarkGray));

    let widths = [
        Constraint::Length(31), Constraint::Length(8), Constraint::Length(21),
        Constraint::Length(21), Constraint::Length(10), Constraint::Length(5),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::bordered().title(format!(" {} files ", filtered.len())))
        .row_highlight_style(Style::default().bg(Color::Rgb(0,80,80)).add_modifier(Modifier::BOLD))
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_symbol(">> ");

    let mut state = ratatui::widgets::TableState::new();
    if app.selected < filtered.len() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let filtered = app.filtered_items();
    let detail = if app.selected < filtered.len() {
        let (_, item) = &filtered[app.selected];
        let mut lines = Vec::new();
        lines.push(Line::from(format!("Path: {}", item.path.display())));
        if let Some(p) = &item.parsed {
            let mut s = format!("Title: {}", p.raw_title);
            if let Some(y) = p.year { s.push_str(&format!(" Year:{y}")); }
            if let Some(s2) = p.season { s.push_str(&format!(" S{s2:02}")); }
            if let Some(e) = p.episode { s.push_str(&format!("E{e:02}")); }
            lines.push(Line::from(s));
        }
        if let Some(q) = &item.quality {
            lines.push(Line::from(format!("Quality: {} {}x{} score={:.1}",
                q.resolution_label, q.width.unwrap_or(0), q.height.unwrap_or(0), q.quality_score)));
        }
        if let Some(s) = &item.scraped {
            lines.push(Line::from(format!("Scraped: {:?} {}", s.source, s.title)));
        }
        lines
    } else {
        vec![Line::from("No item selected")]
    };
    f.render_widget(Paragraph::new(detail).block(Block::bordered().title(" Detail ")).wrap(Wrap{trim:true}), area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let mode_str = match app.mode { Mode::Normal => "NORMAL", Mode::Search => "SEARCH", Mode::Confirm => "CONFIRM" };
    let mode_color = match app.mode { Mode::Normal => Color::Green, Mode::Search => Color::Yellow, Mode::Confirm => Color::Red };
    let status = Line::from(vec![
        Span::styled(format!(" [{mode_str}] "), Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
        Span::raw(&app.status_msg),
        if !app.search_query.is_empty() {
            Span::styled(format!(" /{}", app.search_query), Style::default().fg(Color::Yellow))
        } else { Span::raw("") },
    ]);
    f.render_widget(Paragraph::new(status), area);
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024; const MB: u64 = KB*1024; const GB: u64 = MB*1024;
    if bytes >= GB { format!("{:.1}G", bytes as f64/GB as f64) }
    else if bytes >= MB { format!("{:.1}M", bytes as f64/MB as f64) }
    else if bytes >= KB { format!("{:.0}K", bytes as f64/KB as f64) }
    else { format!("{bytes}B") }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else { let t: String = s.chars().take(max-1).collect(); format!("{t}…") }
}
