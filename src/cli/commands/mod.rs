use crate::core::config::AppConfig;
use crate::core::scanner::Scanner;
use crate::db::cache::Cache;
use crate::models::media::MediaItem;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub mod analyze;
pub mod config;
pub mod dedup;
pub mod organize;
pub mod rename;
pub mod scan;
pub mod scrape;
pub mod tui;

/// Truncate a string to max characters, appending … if truncated
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}

pub fn load_scan_items_or_scan(root: &Path, config: &AppConfig) -> Vec<MediaItem> {
    load_scan_items(root, config).unwrap_or_else(|| {
        let scanner = Scanner::new(config.scan.clone());
        scanner.scan(root)
    })
}

fn load_scan_items(root: &Path, config: &AppConfig) -> Option<Vec<MediaItem>> {
    let root_key = root.to_str()?;
    let cache = Cache::open(&config.cache_path()).ok()?;
    let (updated_at, index) = cache.get_scan_index_entry(root_key)?;
    if is_scan_index_stale(root, updated_at) {
        return None;
    }
    Some(index.items)
}

fn is_scan_index_stale(root: &Path, updated_at: u64) -> bool {
    let root_mtime = root
        .metadata()
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    match root_mtime {
        Some(root_mtime) => root_mtime > updated_at,
        None => false,
    }
}
