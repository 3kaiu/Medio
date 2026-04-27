use crate::core::config::ScanConfig;
use crate::models::media::{MediaItem, MediaType};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Scanner {
    config: ScanConfig,
}

impl Scanner {
    pub fn new(config: ScanConfig) -> Self {
        Self { config }
    }

    /// Scan a directory and return all media items
    pub fn scan(&self, root: &Path) -> Vec<MediaItem> {
        let id_counter = AtomicU64::new(0);
        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {pos} files scanned")
                .unwrap()
        );

        let exclude_dirs = self.config.exclude_dirs.clone();

        let mut builder = WalkBuilder::new(root);
        builder
            .max_depth(Some(self.config.max_depth))
            .follow_links(self.config.follow_symlinks)
            .hidden(false)
            .filter_entry(move |entry| {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if exclude_dirs.iter().any(|d| d == &name) {
                        return false;
                    }
                }
                true
            });

        let mut items: Vec<MediaItem> = Vec::new();

        for result in builder.build() {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }

                    // Get file size
                    let file_size = match std::fs::metadata(path) {
                        Ok(m) => m.len(),
                        Err(_) => continue,
                    };

                    // Skip small files
                    if file_size < self.config.min_file_size {
                        continue;
                    }

                    // Get extension
                    let extension = path
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let media_type = detect_media_type(&extension);

                    // Skip unknown types (not media)
                    if media_type == MediaType::Unknown {
                        continue;
                    }

                    let id = id_counter.fetch_add(1, Ordering::Relaxed);

                    items.push(MediaItem {
                        id,
                        path: path.to_path_buf(),
                        file_size,
                        media_type,
                        extension,
                        parsed: None,
                        quality: None,
                        scraped: None,
                        hash: None,
                        rename_plan: None,
                    });

                    pb.inc(1);
                }
                Err(err) => {
                    tracing::warn!("scan error: {err}");
                }
            }
        }

        pb.finish_with_message(format!("{} media files found", items.len()));
        items
    }
}

/// Detect media type from file extension
fn detect_media_type(ext: &str) -> MediaType {
    match ext.to_lowercase().as_str() {
        // Video (Movie / TvShow — distinguished later by identifier)
        "mkv" | "mp4" | "avi" | "wmv" | "ts" | "mts" | "m2ts" | "flv" | "webm" | "mov" => {
            MediaType::Movie // default, identifier will refine
        }
        // Audio
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "ape" | "wma" | "aac" | "opus" => {
            MediaType::Music
        }
        // Novel / Book
        "epub" | "pdf" | "txt" | "mobi" | "azw3" | "djvu" => MediaType::Novel,
        // STRM
        "strm" => MediaType::Strm,
        _ => MediaType::Unknown,
    }
}
