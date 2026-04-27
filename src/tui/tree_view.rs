use crate::models::media::MediaItem;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem},
    Frame,
};

pub fn draw_tree(f: &mut Frame, items: &[&MediaItem], selected: usize, area: Rect, title: &str) {
    let mut groups: std::collections::BTreeMap<String, Vec<(usize, &MediaItem)>> = std::collections::BTreeMap::new();
    for (idx, item) in items.iter().enumerate() {
        let dir = item.path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".into());
        groups.entry(dir).or_default().push((idx, *item));
    }

    let mut list_items: Vec<ListItem> = Vec::new();
    for (dir, entries) in &groups {
        list_items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("📂 {dir}/"), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
        ])));
        for (idx, item) in entries {
            let name = item.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            let is_sel = *idx == selected;
            let style = if is_sel { Style::default().fg(Color::Cyan) } else { Style::default() };
            list_items.push(ListItem::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(format!("{name}"), style),
            ])));
        }
    }

    let list = List::new(list_items).block(Block::bordered().title(title));
    f.render_widget(list, area);
}
