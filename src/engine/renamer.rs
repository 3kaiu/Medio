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
        let subtitle_plans = self.find_subtitle_plans(&old_path, &new_name);

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
        let mut ctx = HashMap::new();

        // From parsed info
        if let Some(p) = &item.parsed {
            ctx.insert("title", p.raw_title.clone());
            ctx.insert("year", p.year.map(|y| y.to_string()).unwrap_or_default());
            ctx.insert("season", p.season.map(|s| format!("{s:02}")).unwrap_or_default());
            ctx.insert("episode", p.episode.map(|e| format!("{e:02}")).unwrap_or_default());
            ctx.insert("resolution", p.resolution.clone().unwrap_or_default());
            ctx.insert("codec", p.codec.clone().unwrap_or_default());
            ctx.insert("source", p.source.clone().unwrap_or_default());
            ctx.insert("release_group", p.release_group.clone().unwrap_or_default());
            ctx.insert("media_suffix", p.media_suffix.clone().unwrap_or_default());
        }

        // From scraped info
        if let Some(s) = &item.scraped {
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

        // Clean up unresolved placeholders
        result = regex::Regex::new(r"\{\{[^}]+\}\}").ok()?.replace_all(&result, "").to_string();

        // Clean up empty parentheses, dangling separators
        result = regex::Regex::new(r"\(\s*\)").ok()?.replace_all(&result, "").to_string();
        result = regex::Regex::new(r"\[\s*\]").ok()?.replace_all(&result, "").to_string();
        // Collapse multiple spaces
        result = regex::Regex::new(r"  +").ok()?.replace_all(&result, " ").to_string();
        // Remove " - " when surrounded by empty/whitespace, or at edges
        for _ in 0..3 {
            result = regex::Regex::new(r"\s+-\s+").ok()?.replace_all(&result, " - ").to_string();
            result = regex::Regex::new(r"\s+-\s*$").ok()?.replace_all(&result, "").to_string();
            result = regex::Regex::new(r"^\s*-\s+").ok()?.replace_all(&result, "").to_string();
            result = regex::Regex::new(r"  +").ok()?.replace_all(&result, " ").to_string();
        }

        // Clean up double dots, trailing/leading dots/spaces
        result = result.replace("..", ".").trim().to_string();
        let re = regex::Regex::new(r"[\s.]+$").ok()?;
        result = re.replace_all(&result, "").to_string();
        let re2 = regex::Regex::new(r"^[\s.]+").ok()?;
        result = re2.replace_all(&result, "").to_string();

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
                    Ok(()) => actions.push(format!("[renamed] {} → {}", plan.old_path.display(), plan.new_path.display())),
                    Err(e) => actions.push(format!("[error] {} → {}: {e}", plan.old_path.display(), plan.new_path.display())),
                }
                for sub in &plan.subtitle_plans {
                    match std::fs::rename(&sub.old_path, &sub.new_path) {
                        Ok(()) => actions.push(format!("[renamed] {} → {}", sub.old_path.display(), sub.new_path.display())),
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
    use crate::models::media::{MediaItem, MediaType, ParsedInfo, ParseSource};

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
}
