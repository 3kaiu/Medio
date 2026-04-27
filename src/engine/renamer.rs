use crate::core::config::RenameConfig;
use crate::models::media::{MediaItem, RenamePlan, SubtitleRenamePlan};
use std::collections::HashMap;
use std::path::Path;

pub struct Renamer {
    config: RenameConfig,
}

impl Renamer {
    pub fn new(config: RenameConfig) -> Self {
        Self { config }
    }

    /// Generate rename plans for all items
    pub fn plan(&self, items: &[MediaItem]) -> Vec<RenamePlan> {
        items.iter().filter_map(|item| self.plan_one(item)).collect()
    }

    fn plan_one(&self, item: &MediaItem) -> Option<RenamePlan> {
        let template = self.template_for(item);
        let new_name = self.render_template(&template, item)?;
        let old_path = item.path.clone();
        let parent = old_path.parent()?;
        let ext = old_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let new_path = parent.join(format!("{new_name}{ext}"));

        if old_path == new_path {
            return None; // No change needed
        }

        // Find subtitle files
        let subtitle_plans = if self.config.rename_subtitles {
            self.find_subtitle_plans(&old_path, &new_name)
        } else {
            Vec::new()
        };

        Some(RenamePlan {
            old_path,
            new_path,
            subtitle_plans,
        })
    }

    fn template_for(&self, item: &MediaItem) -> String {
        match item.media_type {
            crate::models::media::MediaType::Movie => self.config.movie_template.clone(),
            crate::models::media::MediaType::TvShow => {
                self.config.tv_template.clone()
            }
            crate::models::media::MediaType::Music => self.config.music_template.clone(),
            crate::models::media::MediaType::Novel => self.config.novel_template.clone(),
            crate::models::media::MediaType::Strm => self.config.tv_template.clone(),
            crate::models::media::MediaType::Unknown => self.config.movie_template.clone(),
        }
    }

    fn render_template(&self, template: &str, item: &MediaItem) -> Option<String> {
        use once_cell::sync::Lazy;

        let mut ctx = HashMap::new();
        let season_offset = self.config.season_offset;

        // Base values from parsed info
        if let Some(p) = &item.parsed {
            ctx.insert("title", p.raw_title.clone());
            ctx.insert("year", p.year.map(|y| y.to_string()).unwrap_or_default());
            ctx.insert("season", p.season.map(|s| {
                let adjusted = (s as i32 + season_offset).max(0) as u32;
                format!("{adjusted:02}")
            }).unwrap_or_default());
            ctx.insert("episode", p.episode.map(|e| format!("{e:02}")).unwrap_or_default());
            ctx.insert("resolution", p.resolution.clone().unwrap_or_default());
            ctx.insert("codec", p.codec.clone().unwrap_or_default());
            ctx.insert("source", p.source.clone().unwrap_or_default());
            ctx.insert("release_group", p.release_group.clone().unwrap_or_default());
            ctx.insert("media_suffix", p.media_suffix.clone().unwrap_or_default());
        }

        // Scraped info overrides parsed values when available.
        if let Some(s) = &item.scraped {
            ctx.insert("title", s.title.clone());
            ctx.insert("year", s.year.map(|y| y.to_string()).unwrap_or_else(|| ctx.get("year").cloned().unwrap_or_default()));
            ctx.insert(
                "season",
                s.season_number
                    .map(|season| {
                        let adjusted = (season as i32 + season_offset).max(0) as u32;
                        format!("{adjusted:02}")
                    })
                    .unwrap_or_else(|| ctx.get("season").cloned().unwrap_or_default()),
            );
            ctx.insert(
                "episode",
                s.episode_number
                    .map(|episode| format!("{episode:02}"))
                    .unwrap_or_else(|| ctx.get("episode").cloned().unwrap_or_default()),
            );
            ctx.insert("scraped_title", s.title.clone());
            ctx.insert("episode_name", s.episode_name.clone().unwrap_or_default());
            ctx.insert("artist", s.artist.clone().unwrap_or_default());
            ctx.insert("album", s.album.clone().unwrap_or_default());
            ctx.insert("author", s.author.clone().unwrap_or_default());
        }

        // Simple template rendering (replace {{key}} with value)
        let mut result = template.to_string();
        for (key, value) in &ctx {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }

        let media_suffix = ctx.get("media_suffix").cloned().unwrap_or_default();
        if self.config.preserve_media_suffix
            && !media_suffix.is_empty()
            && !template.contains("{{media_suffix}}")
            && !template.contains("{{ media_suffix }}")
        {
            result = format!("{result} - {media_suffix}");
        }

        // Pre-compiled cleanup regexes
        static RE_UNRESOLVED: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\{\{[^}]+\}\}").unwrap());
        static RE_EMPTY_PAREN: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\(\s*\)").unwrap());
        static RE_EMPTY_BRACKET: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\[\s*\]").unwrap());
        static RE_MULTI_SPACE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"  +").unwrap());
        static RE_DASH_NORMALIZE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\s+-\s+").unwrap());
        static RE_DASH_TRAIL: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\s+-\s*$").unwrap());
        static RE_DASH_LEAD: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^\s*-\s+").unwrap());
        static RE_TRAIL_JUNK: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"[\s.]+$").unwrap());
        static RE_LEAD_JUNK: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^[\s.]+").unwrap());

        // Clean up unresolved placeholders
        result = RE_UNRESOLVED.replace_all(&result, "").to_string();

        // Clean up empty parentheses, dangling separators
        result = RE_EMPTY_PAREN.replace_all(&result, "").to_string();
        result = RE_EMPTY_BRACKET.replace_all(&result, "").to_string();
        // Collapse multiple spaces
        result = RE_MULTI_SPACE.replace_all(&result, " ").to_string();
        // Remove " - " when surrounded by empty/whitespace, or at edges
        for _ in 0..3 {
            result = RE_DASH_NORMALIZE.replace_all(&result, " - ").to_string();
            result = RE_DASH_TRAIL.replace_all(&result, "").to_string();
            result = RE_DASH_LEAD.replace_all(&result, "").to_string();
            result = RE_MULTI_SPACE.replace_all(&result, " ").to_string();
        }

        // Clean up double dots, trailing/leading dots/spaces
        result = result.replace("..", ".").trim().to_string();
        result = RE_TRAIL_JUNK.replace_all(&result, "").to_string();
        result = RE_LEAD_JUNK.replace_all(&result, "").to_string();

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    fn find_subtitle_plans(&self, media_path: &Path, new_name: &str) -> Vec<SubtitleRenamePlan> {
        let dir = match media_path.parent() {
            Some(d) => d,
            None => return Vec::new(),
        };
        let stem = match media_path.file_stem() {
            Some(s) => s.to_string_lossy().to_string(),
            None => return Vec::new(),
        };

        let subtitle_exts = ["srt", "ass", "ssa", "sub", "idx", "vtt"];
        let mut plans = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !subtitle_exts.contains(&ext.as_str()) {
                    continue;
                }
                let sub_stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                // Match: same stem or stem starts with media stem
                if sub_stem == stem || sub_stem.starts_with(&format!("{stem}.")) || sub_stem.starts_with(&format!("{stem}-")) {
                    let suffix = &sub_stem[stem.len()..];
                    let new_sub_name = format!("{new_name}{suffix}");
                    let new_ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                    let new_path = dir.join(format!("{new_sub_name}{new_ext}"));
                    plans.push(SubtitleRenamePlan {
                        old_path: path,
                        new_path,
                    });
                }
            }
        }

        plans
    }

    /// Execute rename plans (dry-run supported)
    pub fn execute(&self, plans: &[RenamePlan], dry_run: bool) -> Vec<String> {
        let mut actions = Vec::new();

        for plan in plans {
            if dry_run {
                actions.push(format!("[dry-run] {} → {}", plan.old_path.display(), plan.new_path.display()));
                for sub in &plan.subtitle_plans {
                    actions.push(format!("[dry-run] {} → {}", sub.old_path.display(), sub.new_path.display()));
                }
            } else {
                match std::fs::rename(&plan.old_path, &plan.new_path) {
                    Ok(()) => {
                        let msg = format!("[renamed] {} → {}", plan.old_path.display(), plan.new_path.display());
                        crate::core::oplog::log(&msg);
                        actions.push(msg);
                    }
                    Err(e) => actions.push(format!("[error] {} → {}: {e}", plan.old_path.display(), plan.new_path.display())),
                }
                for sub in &plan.subtitle_plans {
                    match std::fs::rename(&sub.old_path, &sub.new_path) {
                        Ok(()) => {
                            let msg = format!("[renamed] {} → {}", sub.old_path.display(), sub.new_path.display());
                            crate::core::oplog::log(&msg);
                            actions.push(msg);
                        }
                        Err(e) => actions.push(format!("[error] {} → {}: {e}", sub.old_path.display(), sub.new_path.display())),
                    }
                }
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::RenameConfig;
    use crate::models::media::{MediaItem, MediaType, ParsedInfo, ParseSource, ScrapeResult, ScrapeSource};

    fn make_config() -> RenameConfig {
        RenameConfig::default()
    }

    fn make_movie_item(title: &str, year: Option<u16>) -> MediaItem {
        MediaItem {
            id: 0,
            path: std::path::PathBuf::from(format!("/tmp/{title}.mp4")),
            file_size: 1024,
            media_type: MediaType::Movie,
            extension: "mp4".into(),
            parsed: Some(ParsedInfo {
                raw_title: title.into(),
                year,
                season: None,
                episode: None,
                resolution: Some("1080P".into()),
                codec: Some("H.264".into()),
                source: None,
                release_group: None,
                media_suffix: Some("1080P.H.264".into()),
                parse_source: ParseSource::Regex,
            }),
            hash: None,
            quality: None,
            scraped: None,
            rename_plan: None,
        }
    }

    fn make_tv_item(title: &str, season: u32, episode: u32) -> MediaItem {
        MediaItem {
            id: 0,
            path: std::path::PathBuf::from(format!("/tmp/{title}.S{season:02}E{episode:02}.mp4")),
            file_size: 1024,
            media_type: MediaType::TvShow,
            extension: "mp4".into(),
            parsed: Some(ParsedInfo {
                raw_title: title.into(),
                year: None,
                season: Some(season),
                episode: Some(episode),
                resolution: None,
                codec: None,
                source: None,
                release_group: None,
                media_suffix: None,
                parse_source: ParseSource::Regex,
            }),
            hash: None,
            quality: None,
            scraped: None,
            rename_plan: None,
        }
    }

    fn with_scraped_title(mut item: MediaItem, scraped_title: &str, year: Option<u16>) -> MediaItem {
        item.scraped = Some(ScrapeResult {
            source: ScrapeSource::Tmdb,
            title: scraped_title.into(),
            title_original: None,
            year,
            overview: None,
            rating: None,
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
            tmdb_id: Some(1),
            musicbrainz_id: None,
            openlibrary_id: None,
        });
        item
    }

    fn with_scraped_episode(mut item: MediaItem, title: &str, season: u32, episode: u32) -> MediaItem {
        item.scraped = Some(ScrapeResult {
            source: ScrapeSource::Tmdb,
            title: title.into(),
            title_original: None,
            year: None,
            overview: None,
            rating: None,
            season_number: Some(season),
            episode_number: Some(episode),
            episode_name: Some("Pilot".into()),
            poster_url: None,
            fanart_url: None,
            artist: None,
            album: None,
            track_number: None,
            author: None,
            cover_url: None,
            tmdb_id: Some(1),
            musicbrainz_id: None,
            openlibrary_id: None,
        });
        item
    }

    #[test]
    fn test_movie_template_render() {
        let renamer = Renamer::new(make_config());
        let item = make_movie_item("Inception", Some(2010));
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(new_name.contains("Inception"));
        assert!(new_name.ends_with(".mp4"));
    }

    #[test]
    fn test_tv_template_render() {
        let renamer = Renamer::new(make_config());
        let item = make_tv_item("Breaking Bad", 1, 2);
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(new_name.contains("Breaking Bad"));
        assert!(new_name.contains("S01"));
        assert!(new_name.contains("E02"));
    }

    #[test]
    fn test_empty_year_cleanup() {
        let renamer = Renamer::new(make_config());
        let item = make_movie_item("Test", None);
        let plans = renamer.plan(&[item]);
        if !plans.is_empty() {
            let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
            // Should not have dangling " - " or empty parentheses
            assert!(!new_name.contains("()"));
        }
    }

    #[test]
    fn test_preserve_extension() {
        let renamer = Renamer::new(make_config());
        let item = make_movie_item("Movie", Some(2024));
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(new_name.ends_with(".mp4"));
    }

    #[test]
    fn test_scraped_title_overrides_parsed_title() {
        let renamer = Renamer::new(make_config());
        let item = with_scraped_title(make_movie_item("tt1234567", None), "Inception", Some(2010));
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(new_name.contains("Inception"));
        assert!(!new_name.contains("tt1234567"));
    }

    #[test]
    fn test_preserve_media_suffix_appends_when_template_omits_it() {
        let mut config = make_config();
        config.movie_template = "{{title}}".into();
        let renamer = Renamer::new(config);
        let item = make_movie_item("Inception", Some(2010));
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(new_name.contains("Inception - 1080P.H.264.mp4"));
    }

    #[test]
    fn test_preserve_media_suffix_does_not_duplicate_when_template_includes_it() {
        let mut config = make_config();
        config.movie_template = "{{title}} - {{media_suffix}}".into();
        let renamer = Renamer::new(config);
        let item = make_movie_item("Inception", Some(2010));
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(new_name.matches("1080P.H.264").count(), 1);
    }

    #[test]
    fn test_rename_subtitles_false_skips_subtitle_plans() {
        let dir = tempfile::tempdir().unwrap();
        let video_path = dir.path().join("Show.S01E01.mkv");
        let subtitle_path = dir.path().join("Show.S01E01.srt");
        std::fs::write(&video_path, b"video").unwrap();
        std::fs::write(&subtitle_path, b"subtitle").unwrap();

        let mut item = make_tv_item("Show", 1, 1);
        item.path = video_path;

        let mut config = make_config();
        config.rename_subtitles = false;
        let renamer = Renamer::new(config);
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        assert!(plans[0].subtitle_plans.is_empty());
    }

    #[test]
    fn test_season_offset_applies_to_scraped_season() {
        let mut config = make_config();
        config.season_offset = -1;
        let renamer = Renamer::new(config);
        let item = with_scraped_episode(make_tv_item("Show", 2, 1), "Show", 2, 1);
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0].new_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(new_name.contains("S01E01"));
    }
}
