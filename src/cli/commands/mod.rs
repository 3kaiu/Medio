pub mod scan;
pub mod scrape;
pub mod rename;
pub mod dedup;
pub mod organize;
pub mod analyze;
pub mod tui;
pub mod config;

/// Truncate a string to max characters, appending … if truncated
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}
