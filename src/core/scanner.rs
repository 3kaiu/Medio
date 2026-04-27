use crate::core::config::ScanConfig;
use crate::models::media::{MediaItem, MediaType};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Component, Path};
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
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] {pos} files scanned",
            )
            .unwrap(),
        );

        let exclude_dirs = self.config.exclude_dirs.clone();
        let exclude_path_keywords = self.config.exclude_path_keywords.clone();

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

                    if should_skip_file(path, &extension, &exclude_path_keywords) {
                        continue;
                    }

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

fn should_skip_file(path: &Path, ext: &str, exclude_path_keywords: &[String]) -> bool {
    let normalized = path.to_string_lossy();
    if exclude_path_keywords
        .iter()
        .any(|keyword| normalized.contains(keyword))
    {
        return true;
    }

    let file_stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    let ext = ext.to_ascii_lowercase();

    if matches!(ext.as_str(), "m2ts" | "mts" | "ts") && is_numeric_stem(&file_stem) {
        return true;
    }

    if matches!(ext.as_str(), "m2ts" | "mts" | "ts")
        && path.components().any(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                matches!(
                    name.as_ref(),
                    "BDMV" | "CERTIFICATE" | "STREAM" | "PLAYLIST" | "CLIPINF"
                )
            }
            _ => false,
        })
    {
        return true;
    }

    false
}

fn is_numeric_stem(stem: &str) -> bool {
    !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit())
}

/// Detect media type from file extension
fn detect_media_type(ext: &str) -> MediaType {
    match ext.to_lowercase().as_str() {
        // Video (Movie / TvShow — distinguished later by identifier)
        "mkv" | "mp4" | "avi" | "wmv" | "ts" | "mts" | "m2ts" | "flv" | "webm" | "mov" => {
            MediaType::Movie // default, identifier will refine
        }
        // Audio
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "ape" | "wma" | "aac" | "opus" => MediaType::Music,
        // Novel / Book
        "epub" | "pdf" | "txt" | "mobi" | "azw3" | "djvu" => MediaType::Novel,
        // STRM
        "strm" => MediaType::Strm,
        _ => MediaType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_numeric_stem, should_skip_file};
    use std::path::Path;

    #[test]
    fn skips_bluray_stream_paths() {
        let path = Path::new("/media/Movie/BDMV/STREAM/00001.m2ts");
        assert!(should_skip_file(path, "m2ts", &[]));
    }

    #[test]
    fn skips_numeric_transport_streams() {
        let path = Path::new("/media/raw/00042.ts");
        assert!(should_skip_file(path, "ts", &[]));
        assert!(is_numeric_stem("00042"));
    }

    #[test]
    fn keeps_normal_video_files() {
        let path = Path::new("/media/Show/Season 1/Episode 01.mkv");
        assert!(!should_skip_file(path, "mkv", &[]));
    }

    #[test]
    fn skips_configured_keywords() {
        let path = Path::new("/media/Movie/sample/demo.mp4");
        assert!(should_skip_file(path, "mp4", &[String::from("/sample/")]));
    }
}
