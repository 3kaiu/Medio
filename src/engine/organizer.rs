use crate::core::config::OrganizeConfig;
use crate::core::types::{LinkMode, OrganizeMode};
use crate::engine::nfo_writer;
use crate::models::media::{MediaItem, MediaType};
use crate::scraper::image_scraper;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct OrganizePlan {
    pub source: PathBuf,
    pub target: PathBuf,
    pub action: OrganizeAction,
    pub nfo_content: Option<String>,
    pub image_urls: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizeAction {
    Move,
    #[allow(dead_code)]
    Copy,
    HardLink,
    SymLink,
}

pub struct Organizer {
    config: OrganizeConfig,
}

impl Organizer {
    pub fn new(config: OrganizeConfig) -> Self {
        Self { config }
    }

    /// Generate organize plans for all items
    pub fn plan(&self, items: &[MediaItem], mode: OrganizeMode, link: LinkMode) -> Vec<OrganizePlan> {
        let root = if self.config.root.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            self.config.root.clone()
        };

        items.iter().filter_map(|item| {
            let target_dir = self.target_dir(item, &root, &mode)?;
            let filename = item.path.file_name()?.to_os_string();
            let target = target_dir.join(&filename);

            // Skip if already in the right place
            if item.path.parent() == Some(&target_dir) {
                return None;
            }

            let action = match link {
                LinkMode::None => match mode {
                    OrganizeMode::Archive | OrganizeMode::Local => OrganizeAction::Move,
                    OrganizeMode::Rename => OrganizeAction::Move,
                },
                LinkMode::Hard => OrganizeAction::HardLink,
                LinkMode::Sym => OrganizeAction::SymLink,
            };

            // NFO content
            let nfo_content = if self.config.with_nfo {
                nfo_writer::generate(item)
            } else {
                None
            };

            // Image URLs from scraped data
            let image_urls = if self.config.with_images {
                item.scraped
                    .as_ref()
                    .map(image_scraper::collect_urls)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            Some(OrganizePlan {
                source: item.path.clone(),
                target,
                action,
                nfo_content,
                image_urls,
            })
        }).collect()
    }

    fn target_dir(&self, item: &MediaItem, root: &Path, mode: &OrganizeMode) -> Option<PathBuf> {
        match mode {
            OrganizeMode::Rename => {
                // Just rename in place, no directory reorganization
                item.path.parent().map(|p| p.to_path_buf())
            }
            OrganizeMode::Archive => {
                // Organize into: root/MediaType/Title/Season/
                let type_dir = match item.media_type {
                    MediaType::Movie => "Movies",
                    MediaType::TvShow => "TV Shows",
                    MediaType::Music => "Music",
                    MediaType::Novel => "Books",
                    MediaType::Strm => "TV Shows",
                    MediaType::Unknown => "Other",
                };

                let title = item.scraped.as_ref()
                    .map(|s| sanitize_filename(&s.title))
                    .or_else(|| item.parsed.as_ref().map(|p| sanitize_filename(&p.raw_title)))
                    .unwrap_or_else(|| "Unknown".into());

                let mut dir = root.join(type_dir).join(&title);

                // Add season subdirectory for TV
                if item.media_type == MediaType::TvShow {
                    if let Some(s) = item.parsed.as_ref().and_then(|p| p.season) {
                        dir = dir.join(format!("Season {s:02}"));
                    }
                }

                // Add artist/album for music
                if item.media_type == MediaType::Music {
                    if let Some(artist) = item.scraped.as_ref().and_then(|s| s.artist.as_ref()) {
                        dir = root.join(type_dir).join(sanitize_filename(artist)).join(&title);
                    }
                }

                Some(dir)
            }
            OrganizeMode::Local => {
                // Organize within the same parent directory
                let parent = item.path.parent()?;
                let title = item.scraped.as_ref()
                    .map(|s| sanitize_filename(&s.title))
                    .or_else(|| item.parsed.as_ref().map(|p| sanitize_filename(&p.raw_title)));

                if let Some(title) = title {
                    let mut dir = parent.join(&title);
                    if item.media_type == MediaType::TvShow {
                        if let Some(s) = item.parsed.as_ref().and_then(|p| p.season) {
                            dir = dir.join(format!("Season {s:02}"));
                        }
                    }
                    Some(dir)
                } else {
                    None
                }
            }
        }
    }

    /// Execute organize plans (dry-run supported)
    pub fn execute(&self, plans: &[OrganizePlan], dry_run: bool) -> Vec<String> {
        let mut actions = Vec::new();

        // Reuse HTTP client for all image downloads
        let img_client = if plans.iter().any(|p| !p.image_urls.is_empty()) {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .ok()
        } else {
            None
        };

        for plan in plans {
            // Create target directory
            if let Some(parent) = plan.target.parent() {
                if dry_run {
                    actions.push(format!("[dry-run] mkdir -p {}", parent.display()));
                } else if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        actions.push(format!("[error] mkdir {}: {e}", parent.display()));
                        continue;
                    }
                }
            }

            // Move/copy/link file
            let action_label = match plan.action {
                OrganizeAction::Move => "move",
                OrganizeAction::Copy => "copy",
                OrganizeAction::HardLink => "hardlink",
                OrganizeAction::SymLink => "symlink",
            };

            if dry_run {
                actions.push(format!("[dry-run] {action_label} {} → {}", plan.source.display(), plan.target.display()));
            } else {
                let result = match plan.action {
                    OrganizeAction::Move => std::fs::rename(&plan.source, &plan.target),
                    OrganizeAction::Copy => std::fs::copy(&plan.source, &plan.target).map(|_| ()),
                    OrganizeAction::HardLink => std::fs::hard_link(&plan.source, &plan.target),
                    OrganizeAction::SymLink => {
                        if plan.target.exists() {
                            Ok(())
                        } else {
                            std::os::unix::fs::symlink(&plan.source, &plan.target)
                        }
                    }
                };
                match result {
                    Ok(()) => {
                        let msg = format!("[{action_label}] {} → {}", plan.source.display(), plan.target.display());
                        crate::core::oplog::log(&msg);
                        actions.push(msg);
                    }
                    Err(e) => actions.push(format!("[error] {} → {}: {e}", plan.source.display(), plan.target.display())),
                }
            }

            // Write NFO
            if let Some(nfo) = &plan.nfo_content {
                let nfo_path = plan.target.with_extension("nfo");
                if dry_run {
                    actions.push(format!("[dry-run] write nfo {}", nfo_path.display()));
                } else if let Err(e) = std::fs::write(&nfo_path, nfo) {
                    actions.push(format!("[error] write nfo {}: {e}", nfo_path.display()));
                } else {
                    actions.push(format!("[nfo] {}", nfo_path.display()));
                }
            }

            // Download images
            for (idx, url) in plan.image_urls.iter().enumerate() {
                let img_dir = plan.target.parent().unwrap_or(Path::new("."));
                let img_path = image_scraper::build_image_path(img_dir, idx, url);

                if dry_run {
                    actions.push(format!("[dry-run] download image from {url}"));
                } else if let Some(ref client) = img_client {
                    match image_scraper::download(client, url, &img_path) {
                        Ok(()) => actions.push(format!("[image] {}", img_path.display())),
                        Err(e) => actions.push(format!("[error] {e}")),
                    }
                }
            }
        }

        // Cleanup empty directories
        if self.config.cleanup_empty_dirs && !dry_run {
            let mut dirs: Vec<PathBuf> = plans.iter().filter_map(|p| p.source.parent().map(|d| d.to_path_buf())).collect();
            dirs.sort();
            dirs.dedup();
            for dir in dirs.iter().rev() {
                if dir.exists() && std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
                    if let Ok(()) = std::fs::remove_dir(dir) {
                        actions.push(format!("[cleanup] removed empty dir {}", dir.display()));
                    }
                }
            }
        }

        actions
    }
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::OrganizeConfig;
    use crate::models::media::{MediaItem, MediaType, ParsedInfo, ParseSource, ScrapeResult, ScrapeSource};

    fn make_org_config() -> OrganizeConfig {
        OrganizeConfig {
            mode: OrganizeMode::Archive,
            root: std::path::PathBuf::from("/tmp/medio_test"),
            link_mode: LinkMode::None,
            with_nfo: false,
            with_images: false,
            cleanup_empty_dirs: false,
        }
    }

    fn make_movie_item(title: &str) -> MediaItem {
        MediaItem {
            id: 0,
            path: std::path::PathBuf::from(format!("/tmp/source/{title}.mp4")),
            file_size: 1024,
            media_type: MediaType::Movie,
            extension: "mp4".into(),
            parsed: Some(ParsedInfo {
                raw_title: title.into(),
                year: Some(2024),
                season: None,
                episode: None,
                resolution: None,
                codec: None,
                source: None,
                release_group: None,
                media_suffix: None,
                parse_source: ParseSource::Regex,
            }),
            scraped: Some(ScrapeResult {
                source: ScrapeSource::Tmdb,
                title: title.into(),
                title_original: None,
                year: Some(2024),
                overview: None,
                rating: Some(8.0),
                season_number: None,
                episode_number: None,
                episode_name: None,
                poster_url: None,
                fanart_url: None,
                artist: None,
                album: None,
                track_number: None,
                author: None,
                cover_url: None,
                tmdb_id: Some(123),
                musicbrainz_id: None,
                openlibrary_id: None,
            }),
            hash: None,
            quality: None,
            rename_plan: None,
        }
    }

    fn make_tv_item(title: &str, season: u32) -> MediaItem {
        MediaItem {
            id: 0,
            path: std::path::PathBuf::from(format!("/tmp/source/{title}.S{season:02}E01.mp4")),
            file_size: 1024,
            media_type: MediaType::TvShow,
            extension: "mp4".into(),
            parsed: Some(ParsedInfo {
                raw_title: title.into(),
                year: None,
                season: Some(season),
                episode: Some(1),
                resolution: None,
                codec: None,
                source: None,
                release_group: None,
                media_suffix: None,
                parse_source: ParseSource::Regex,
            }),
            scraped: None,
            hash: None,
            quality: None,
            rename_plan: None,
        }
    }

    #[test]
    fn test_archive_mode_movie() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].target.to_string_lossy().contains("Movies"));
        assert!(plans[0].target.to_string_lossy().contains("Inception"));
    }

    #[test]
    fn test_archive_mode_tv_with_season() {
        let organizer = Organizer::new(make_org_config());
        let item = make_tv_item("Breaking Bad", 2);
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].target.to_string_lossy().contains("TV Shows"));
        assert!(plans[0].target.to_string_lossy().contains("Season 02"));
    }

    #[test]
    fn test_rename_mode_same_dir() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Test");
        let plans = organizer.plan(&[item], OrganizeMode::Rename, LinkMode::None);
        // Rename mode keeps same parent directory
        if !plans.is_empty() {
            assert_eq!(plans[0].target.parent(), plans[0].source.parent());
        }
    }

    #[test]
    fn test_symlink_mode() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Test");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::Sym);
        assert!(!plans.is_empty());
        assert_eq!(plans[0].action, OrganizeAction::SymLink);
    }

    #[test]
    fn test_hardlink_mode() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Test");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::Hard);
        assert!(!plans.is_empty());
        assert_eq!(plans[0].action, OrganizeAction::HardLink);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Movie: A/B"), "Movie_ A_B");
        assert_eq!(sanitize_filename("Clean Title"), "Clean Title");
        assert_eq!(sanitize_filename("What?<>*"), "What____");
    }

    #[test]
    fn test_nfo_generation() {
        let mut config = make_org_config();
        config.with_nfo = true;
        let organizer = Organizer::new(config);
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].nfo_content.is_some());
        let nfo = plans[0].nfo_content.as_ref().unwrap();
        assert!(nfo.contains("<movie>"));
        assert!(nfo.contains("<title>Inception</title>"));
    }

    #[test]
    fn test_images_do_not_force_nfo_generation() {
        let mut config = make_org_config();
        config.with_images = true;
        let organizer = Organizer::new(config);
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].nfo_content.is_none());
    }
}
