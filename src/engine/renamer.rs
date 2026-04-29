use crate::core::config::RenameConfig;
use crate::models::media::{
    DirectoryRenamePlan, MediaItem, MediaType, RenamePlan, ScrapeSource, SubtitleRenamePlan,
};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tera::{Context, Tera};

pub struct Renamer {
    config: RenameConfig,
}

impl Renamer {
    pub fn new(config: RenameConfig) -> Self {
        Self { config }
    }

    /// Generate rename plans for all items
    pub fn plan(&self, items: &[MediaItem]) -> Vec<RenamePlan> {
        let mut plans: Vec<RenamePlan> = items
            .iter()
            .filter_map(|item| self.plan_one(item))
            .collect();

        let directory_plans = self.build_directory_plans(items);
        if !directory_plans.is_empty() {
            let by_source: HashMap<_, _> = directory_plans
                .into_iter()
                .map(|plan| (plan.old_path.clone(), plan))
                .collect();
            let mut assigned = HashSet::new();

            for plan in &mut plans {
                let mut attached = Vec::new();
                let mut seen = HashSet::new();
                for dir in parent_chain(&plan.old_path) {
                    if let Some(dir_plan) = by_source.get(dir)
                        && seen.insert(dir_plan.old_path.clone())
                    {
                        assigned.insert(dir_plan.old_path.clone());
                        attached.push(dir_plan.clone());
                    }
                }
                attached.sort_by_key(|plan| std::cmp::Reverse(plan.old_path.components().count()));
                plan.directory_plans = attached;
            }

            for dir_plan in by_source.values() {
                if assigned.contains(&dir_plan.old_path) {
                    continue;
                }
                plans.push(RenamePlan {
                    old_path: dir_plan.old_path.clone(),
                    new_path: dir_plan.old_path.clone(),
                    subtitle_plans: Vec::new(),
                    directory_plans: vec![dir_plan.clone()],
                });
            }
        }

        plans
    }

    fn plan_one(&self, item: &MediaItem) -> Option<RenamePlan> {
        let template = self.template_for(item);
        let rendered_name = self.render_template(&template, item)?;
        let old_path = item.path.clone();
        let parent = old_path.parent()?;
        let ext = old_path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let new_file_name = if template_mentions_ext(&template) {
            rendered_name.clone()
        } else {
            format!("{rendered_name}{ext}")
        };
        let new_path = parent.join(&new_file_name);

        if old_path == new_path {
            return None; // No change needed
        }

        // Find subtitle files
        let subtitle_plans = if self.config.rename_subtitles {
            let subtitle_name = if template_mentions_ext(&template) {
                std::path::Path::new(&rendered_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(rendered_name.clone())
            } else {
                rendered_name.clone()
            };
            self.find_subtitle_plans(&old_path, &subtitle_name)
        } else {
            Vec::new()
        };

        Some(RenamePlan {
            old_path,
            new_path,
            subtitle_plans,
            directory_plans: Vec::new(),
        })
    }

    fn build_directory_plans(&self, items: &[MediaItem]) -> Vec<DirectoryRenamePlan> {
        let mut plans = Vec::new();
        let mut seen = HashSet::new();

        for item in items {
            match item.media_type {
                MediaType::TvShow | MediaType::Strm => {
                    if let Some(plan) = tv_season_directory_plan(item) {
                        push_directory_plan(&mut plans, &mut seen, plan);
                    }
                    if let Some(plan) = tv_show_directory_plan(item) {
                        push_directory_plan(&mut plans, &mut seen, plan);
                    }
                }
                MediaType::Movie => {
                    if let Some(plan) = movie_directory_plan(item) {
                        push_directory_plan(&mut plans, &mut seen, plan);
                    }
                }
                _ => {}
            }
        }

        plans
    }

    fn template_for(&self, item: &MediaItem) -> String {
        match item.media_type {
            crate::models::media::MediaType::Movie => self.config.movie_template.clone(),
            crate::models::media::MediaType::TvShow => self.config.tv_template.clone(),
            crate::models::media::MediaType::Music => self.config.music_template.clone(),
            crate::models::media::MediaType::Novel => self.config.novel_template.clone(),
            crate::models::media::MediaType::Strm => self.config.tv_template.clone(),
            crate::models::media::MediaType::Unknown => self.config.movie_template.clone(),
        }
    }

    fn render_template(&self, template: &str, item: &MediaItem) -> Option<String> {
        let mut ctx: HashMap<String, String> = HashMap::new();
        let season_offset = self.config.season_offset;
        let ext = item
            .path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();

        // Base values from parsed info
        if let Some(p) = &item.parsed {
            ctx.insert("title".into(), p.raw_title.clone());
            ctx.insert(
                "year".into(),
                p.year.map(|y| y.to_string()).unwrap_or_default(),
            );
            ctx.insert(
                "season".into(),
                p.season
                    .map(|s| {
                        let adjusted = (s as i32 + season_offset).max(0) as u32;
                        format!("{adjusted:02}")
                    })
                    .unwrap_or_default(),
            );
            ctx.insert(
                "s".into(),
                p.season
                    .map(|s| {
                        let adjusted = (s as i32 + season_offset).max(0) as u32;
                        format!("{adjusted:02}")
                    })
                    .unwrap_or_default(),
            );
            ctx.insert(
                "episode".into(),
                p.episode.map(|e| format!("{e:02}")).unwrap_or_default(),
            );
            ctx.insert(
                "e".into(),
                p.episode.map(|e| format!("{e:02}")).unwrap_or_default(),
            );
            ctx.insert(
                "resolution".into(),
                p.resolution.clone().unwrap_or_default(),
            );
            ctx.insert("codec".into(), p.codec.clone().unwrap_or_default());
            ctx.insert("source".into(), p.source.clone().unwrap_or_default());
            ctx.insert(
                "release_group".into(),
                p.release_group.clone().unwrap_or_default(),
            );
            ctx.insert(
                "media_suffix".into(),
                p.media_suffix.clone().unwrap_or_default(),
            );
            ctx.insert(
                "parse_source".into(),
                format!("{:?}", p.parse_source).to_lowercase(),
            );
        }

        // Scraped info overrides parsed values when available.
        if let Some(s) = &item.scraped {
            ctx.insert("title".into(), s.title.clone());
            ctx.insert(
                "year".into(),
                s.year
                    .map(|y| y.to_string())
                    .unwrap_or_else(|| ctx.get("year").cloned().unwrap_or_default()),
            );
            ctx.insert(
                "season".into(),
                s.season_number
                    .map(|season| {
                        let adjusted = (season as i32 + season_offset).max(0) as u32;
                        format!("{adjusted:02}")
                    })
                    .unwrap_or_else(|| ctx.get("season").cloned().unwrap_or_default()),
            );
            ctx.insert(
                "s".into(),
                s.season_number
                    .map(|season| {
                        let adjusted = (season as i32 + season_offset).max(0) as u32;
                        format!("{adjusted:02}")
                    })
                    .unwrap_or_else(|| ctx.get("s").cloned().unwrap_or_default()),
            );
            ctx.insert(
                "episode".into(),
                s.episode_number
                    .map(|episode| format!("{episode:02}"))
                    .unwrap_or_else(|| ctx.get("episode").cloned().unwrap_or_default()),
            );
            ctx.insert(
                "e".into(),
                s.episode_number
                    .map(|episode| format!("{episode:02}"))
                    .unwrap_or_else(|| ctx.get("e").cloned().unwrap_or_default()),
            );
            ctx.insert("scraped_title".into(), s.title.clone());
            ctx.insert(
                "episode_name".into(),
                s.episode_name.clone().unwrap_or_default(),
            );
            ctx.insert("ep_name".into(), s.episode_name.clone().unwrap_or_default());
            ctx.insert("artist".into(), s.artist.clone().unwrap_or_default());
            ctx.insert("album".into(), s.album.clone().unwrap_or_default());
            ctx.insert("author".into(), s.author.clone().unwrap_or_default());
            let provider = match s.source {
                ScrapeSource::Tmdb => "tmdb",
                ScrapeSource::MusicBrainz => "musicbrainz",
                ScrapeSource::OpenLibrary => "openlibrary",
                ScrapeSource::AiAssist => "ai",
                ScrapeSource::LocalNfo => "localnfo",
                ScrapeSource::Guess => "guess",
            };
            ctx.insert("source_provider".into(), provider.into());
            ctx.insert("provider".into(), provider.into());
            let media_id = s
                .tmdb_id
                .map(|id| id.to_string())
                .or_else(|| s.musicbrainz_id.clone())
                .or_else(|| s.openlibrary_id.clone())
                .unwrap_or_default();
            ctx.insert("media_id".into(), media_id.clone());
            ctx.insert(
                "tmdbid".into(),
                s.tmdb_id.map(|id| id.to_string()).unwrap_or_default(),
            );
            ctx.insert("bgmid".into(), String::new());
        }
        ctx.entry("episode_name".into()).or_default();
        ctx.entry("ep_name".into()).or_default();
        ctx.entry("artist".into()).or_default();
        ctx.entry("album".into()).or_default();
        ctx.entry("author".into()).or_default();
        ctx.entry("source_provider".into()).or_default();
        ctx.entry("provider".into()).or_default();
        ctx.entry("media_id".into()).or_default();
        ctx.entry("tmdbid".into()).or_default();
        ctx.entry("bgmid".into()).or_default();
        ctx.entry("parse_source".into()).or_default();
        ctx.insert("ext".into(), ext.clone());
        ctx.insert(
            "is_tv".into(),
            matches!(item.media_type, crate::models::media::MediaType::TvShow).to_string(),
        );

        let result = if is_advanced_template(template) {
            render_advanced_template(template, &ctx, self.config.preserve_media_suffix).ok()?
        } else {
            render_legacy_template(template, &ctx, self.config.preserve_media_suffix)
        };

        // Pre-compiled cleanup regexes
        static RE_EMPTY_PAREN: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"\(\s*\)").unwrap());
        static RE_EMPTY_BRACKET: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"\[\s*\]").unwrap());
        static RE_MULTI_SPACE: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"  +").unwrap());
        static RE_DASH_NORMALIZE: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"\s+-\s+").unwrap());
        static RE_DASH_TRAIL: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"\s+-\s*$").unwrap());
        static RE_DASH_LEAD: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"^\s*-\s+").unwrap());
        static RE_EMPTY_SEASON_EP: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"(^|[^A-Za-z0-9])S(E\d{2}\b)").unwrap());
        static RE_TRAIL_JUNK: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"[\s.]+$").unwrap());
        static RE_LEAD_JUNK: Lazy<regex::Regex> =
            Lazy::new(|| regex::Regex::new(r"^[\s.]+").unwrap());

        let mut result = result;

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

        result = RE_EMPTY_SEASON_EP.replace_all(&result, "$1$2").to_string();

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
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !subtitle_exts.contains(&ext.as_str()) {
                    continue;
                }
                let sub_stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                // Match: same stem or stem starts with media stem
                if sub_stem == stem
                    || sub_stem.starts_with(&format!("{stem}."))
                    || sub_stem.starts_with(&format!("{stem}-"))
                {
                    let suffix = &sub_stem[stem.len()..];
                    let new_sub_name = format!("{new_name}{suffix}");
                    let new_ext = path
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
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
                if plan.old_path != plan.new_path {
                    actions.push(format!(
                        "[dry-run] {} → {}",
                        plan.old_path.display(),
                        plan.new_path.display()
                    ));
                }
                for sub in &plan.subtitle_plans {
                    actions.push(format!(
                        "[dry-run] {} → {}",
                        sub.old_path.display(),
                        sub.new_path.display()
                    ));
                }
                for dir in &plan.directory_plans {
                    actions.push(format!(
                        "[dry-run] {} → {}",
                        dir.old_path.display(),
                        dir.new_path.display()
                    ));
                }
            } else {
                if plan.old_path != plan.new_path {
                    match std::fs::rename(&plan.old_path, &plan.new_path) {
                        Ok(()) => {
                            let msg = format!(
                                "[renamed] {} → {}",
                                plan.old_path.display(),
                                plan.new_path.display()
                            );
                            crate::core::oplog::log(&msg);
                            actions.push(msg);
                        }
                        Err(e) => actions.push(format!(
                            "[error] {} → {}: {e}",
                            plan.old_path.display(),
                            plan.new_path.display()
                        )),
                    }
                }
                for sub in &plan.subtitle_plans {
                    match std::fs::rename(&sub.old_path, &sub.new_path) {
                        Ok(()) => {
                            let msg = format!(
                                "[renamed] {} → {}",
                                sub.old_path.display(),
                                sub.new_path.display()
                            );
                            crate::core::oplog::log(&msg);
                            actions.push(msg);
                        }
                        Err(e) => actions.push(format!(
                            "[error] {} → {}: {e}",
                            sub.old_path.display(),
                            sub.new_path.display()
                        )),
                    }
                }
                for dir in &plan.directory_plans {
                    if !dir.old_path.exists() || dir.new_path.exists() {
                        continue;
                    }
                    match std::fs::rename(&dir.old_path, &dir.new_path) {
                        Ok(()) => {
                            let msg = format!(
                                "[renamed] {} → {}",
                                dir.old_path.display(),
                                dir.new_path.display()
                            );
                            crate::core::oplog::log(&msg);
                            actions.push(msg);
                        }
                        Err(e) => actions.push(format!(
                            "[error] {} → {}: {e}",
                            dir.old_path.display(),
                            dir.new_path.display()
                        )),
                    }
                }
            }
        }

        actions
    }
}

fn push_directory_plan(
    plans: &mut Vec<DirectoryRenamePlan>,
    seen: &mut HashSet<std::path::PathBuf>,
    plan: DirectoryRenamePlan,
) {
    if seen.insert(plan.old_path.clone()) {
        plans.push(plan);
    }
}

fn parent_chain(path: &Path) -> Vec<&Path> {
    let mut dirs = Vec::new();
    let mut current = path.parent();
    while let Some(dir) = current {
        dirs.push(dir);
        current = dir.parent();
    }
    dirs
}

fn title_for(item: &MediaItem) -> Option<String> {
    item.scraped
        .as_ref()
        .map(|s| s.title.clone())
        .or_else(|| item.parsed.as_ref().map(|p| p.raw_title.clone()))
        .map(|title| sanitize_name(&title))
        .filter(|title| !title.is_empty())
}

fn tv_season_directory_plan(item: &MediaItem) -> Option<DirectoryRenamePlan> {
    let parsed = item.parsed.as_ref()?;
    let season = parsed.season?;
    let old_path = item.path.parent()?.to_path_buf();
    let desired_name = format!("Season {season:02}");
    rename_dir_plan_if_needed(&old_path, &desired_name)
}

fn tv_show_directory_plan(item: &MediaItem) -> Option<DirectoryRenamePlan> {
    let parsed = item.parsed.as_ref()?;
    let old_season_dir = item.path.parent()?;
    let old_show_dir = old_season_dir.parent()?;
    let mut desired_name = title_for(item)?;
    if desired_name.is_empty() {
        desired_name = parsed.raw_title.clone();
    }
    rename_dir_plan_if_needed(old_show_dir, &desired_name)
}

fn movie_directory_plan(item: &MediaItem) -> Option<DirectoryRenamePlan> {
    let old_path = item.path.parent()?;
    if is_tv_extras_directory(old_path) {
        return None;
    }
    let title = title_for(item)?;
    let year = item
        .scraped
        .as_ref()
        .and_then(|s| s.year)
        .or_else(|| item.parsed.as_ref().and_then(|p| p.year));
    let desired_name = match year {
        Some(year) => format!("{title} ({year})"),
        None => title,
    };
    rename_dir_plan_if_needed(old_path, &desired_name)
}

fn rename_dir_plan_if_needed(old_path: &Path, desired_name: &str) -> Option<DirectoryRenamePlan> {
    let desired_name = sanitize_name(desired_name);
    if desired_name.is_empty() {
        return None;
    }
    let current_name = old_path.file_name()?.to_string_lossy().to_string();
    if current_name == desired_name {
        return None;
    }
    let new_path = old_path.parent()?.join(desired_name);
    if new_path == old_path {
        return None;
    }
    Some(DirectoryRenamePlan {
        old_path: old_path.to_path_buf(),
        new_path,
    })
}

fn is_tv_extras_directory(dir: &Path) -> bool {
    let Some(parent) = dir.parent() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return false;
    };

    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        entry.path().is_dir() && is_seasonish_dir_name(&name)
    })
}

fn is_seasonish_dir_name(name: &str) -> bool {
    static RE_SEASONISH: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"(?i)^(?:S\d{1,2}|Season\s+\d{1,2})$").unwrap());
    RE_SEASONISH.is_match(name.trim())
}

fn sanitize_name(s: &str) -> String {
    static RE_MULTI_SPACE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\s+").unwrap());

    let cleaned = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>();

    RE_MULTI_SPACE.replace_all(cleaned.trim(), " ").to_string()
}

fn template_mentions_ext(template: &str) -> bool {
    template.contains("{ext}") || template.contains("{{ext") || template.contains("{{ ext")
}

fn is_advanced_template(template: &str) -> bool {
    template.contains("{{") || template.contains("{%") || template.contains("{#")
}

fn render_legacy_template(
    template: &str,
    ctx: &HashMap<String, String>,
    preserve_media_suffix: bool,
) -> String {
    let mut rendered = template.to_string();
    let media_suffix = ctx.get("media_suffix").cloned().unwrap_or_default();
    if preserve_media_suffix && !media_suffix.is_empty() && !template.contains("{media_suffix}") {
        rendered.push_str(" - {media_suffix}");
    }

    let replacements = [
        ("title", "title"),
        ("year", "year"),
        ("season", "season"),
        ("episode", "episode"),
        ("s:02d", "s"),
        ("s", "s"),
        ("e:02d", "e"),
        ("e", "e"),
        ("ep_name", "ep_name"),
        ("episode_name", "episode_name"),
        ("artist", "artist"),
        ("album", "album"),
        ("author", "author"),
        ("media_suffix", "media_suffix"),
        ("ext", "ext"),
        ("parse_source", "parse_source"),
        ("source_provider", "source_provider"),
        ("provider", "provider"),
        ("media_id", "media_id"),
        ("tmdbid", "tmdbid"),
        ("bgmid", "bgmid"),
    ];

    for (placeholder, key) in replacements {
        rendered = rendered.replace(
            &format!("{{{placeholder}}}"),
            ctx.get(key).map(String::as_str).unwrap_or(""),
        );
    }

    rendered
}

fn render_advanced_template(
    template: &str,
    ctx: &HashMap<String, String>,
    preserve_media_suffix: bool,
) -> Result<String, tera::Error> {
    static RE_EXT: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"\{\{\s*ext\s*\}\}").unwrap());

    let media_suffix = ctx.get("media_suffix").cloned().unwrap_or_default();
    let mut working = template.to_string();
    if preserve_media_suffix && !media_suffix.is_empty() && !working.contains("media_suffix") {
        if RE_EXT.is_match(&working) {
            working = RE_EXT
                .replace(&working, " - {{ media_suffix }}{{ ext }}")
                .to_string();
        } else {
            working.push_str(" - {{ media_suffix }}");
        }
    }

    let mut context = Context::new();
    for (key, value) in ctx {
        context.insert(key, value);
    }
    context.insert(
        "is_tv",
        &(ctx.get("is_tv").map(|v| v == "true").unwrap_or(false)),
    );

    Tera::one_off(&working, &context, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::RenameConfig;
    use crate::models::media::{
        MediaItem, MediaType, ParseSource, ParsedInfo, ScrapeResult, ScrapeSource,
    };

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

    fn with_scraped_title(
        mut item: MediaItem,
        scraped_title: &str,
        year: Option<u16>,
    ) -> MediaItem {
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

    fn with_scraped_episode(
        mut item: MediaItem,
        title: &str,
        season: u32,
        episode: u32,
    ) -> MediaItem {
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
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(new_name.contains("Inception"));
        assert!(new_name.ends_with(".mp4"));
    }

    #[test]
    fn test_tv_template_render() {
        let renamer = Renamer::new(make_config());
        let item = make_tv_item("Breaking Bad", 1, 2);
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(new_name.contains("Breaking Bad"));
        assert!(new_name.contains("S01"));
        assert!(new_name.contains("E02"));
    }

    #[test]
    fn test_tv_template_render_episode_only() {
        let renamer = Renamer::new(make_config());
        let mut item = make_tv_item("财阀家的小儿子", 1, 3);
        if let Some(parsed) = &mut item.parsed {
            parsed.season = None;
        }
        let item = with_scraped_episode(item, "财阀家的小儿子", 0, 3);
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(new_name.contains("E03"));
        assert!(!new_name.contains("SE03"));
    }

    #[test]
    fn test_legacy_template_variables_match_media_renamer_ai_style() {
        let mut config = make_config();
        config.movie_template = "{title} ({year}) - {media_suffix}".into();
        let renamer = Renamer::new(config);
        let item = with_scraped_title(
            make_movie_item("Inception", Some(2010)),
            "盗梦空间",
            Some(2010),
        );
        let plans = renamer.plan(&[item]);
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(new_name, "盗梦空间 (2010) - 1080P.H.264.mp4");
    }

    #[test]
    fn test_advanced_template_supports_conditionals_and_ext() {
        let mut config = make_config();
        config.tv_template = "{{ title }} - S{{ season }}E{{ episode }}{% if ep_name %} - {{ ep_name }}{% endif %}{{ ext }}".into();
        config.preserve_media_suffix = false;
        let renamer = Renamer::new(config);
        let item = with_scraped_episode(make_tv_item("Breaking Bad", 1, 2), "绝命毒师", 1, 2);
        let plans = renamer.plan(&[item]);
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(new_name, "绝命毒师 - S01E02 - Pilot.mp4");
    }

    #[test]
    fn test_empty_year_cleanup() {
        let renamer = Renamer::new(make_config());
        let item = make_movie_item("Test", None);
        let plans = renamer.plan(&[item]);
        if !plans.is_empty() {
            let new_name = plans[0]
                .new_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
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
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(new_name.ends_with(".mp4"));
    }

    #[test]
    fn test_scraped_title_overrides_parsed_title() {
        let renamer = Renamer::new(make_config());
        let item = with_scraped_title(make_movie_item("tt1234567", None), "Inception", Some(2010));
        let plans = renamer.plan(&[item]);
        assert!(!plans.is_empty());
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
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
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
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
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
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
        let new_name = plans[0]
            .new_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(new_name.contains("S01E01"));
    }

    #[test]
    fn test_tv_directory_plans_include_season_and_show_root() {
        let renamer = Renamer::new(make_config());
        let mut item = with_scraped_episode(make_tv_item("Noise", 1, 2), "9号秘事", 1, 2);
        item.path = std::path::PathBuf::from("/tmp/9号秘事 1-9季(1)/S01/Noise.S01E02.mp4");

        let plans = renamer.plan(&[item]);
        assert_eq!(plans.len(), 1);
        let dir_plans = &plans[0].directory_plans;
        assert_eq!(dir_plans.len(), 2);
        assert_eq!(
            dir_plans[0].new_path,
            std::path::PathBuf::from("/tmp/9号秘事 1-9季(1)/Season 01")
        );
        assert_eq!(
            dir_plans[1].new_path,
            std::path::PathBuf::from("/tmp/9号秘事")
        );
    }

    #[test]
    fn test_movie_directory_plan_uses_title_and_year() {
        let renamer = Renamer::new(make_config());
        let mut item = with_scraped_title(
            make_movie_item("noise", Some(2025)),
            "刺杀小说家2",
            Some(2025),
        );
        item.path = std::path::PathBuf::from("/tmp/刺z杀z小z说家2 (2025) 4K 高码 HDR/noise.mkv");

        let plans = renamer.plan(&[item]);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].directory_plans.len(), 1);
        assert_eq!(
            plans[0].directory_plans[0].new_path,
            std::path::PathBuf::from("/tmp/刺杀小说家2 (2025)")
        );
    }

    #[test]
    fn test_directory_only_rename_still_generates_plan() {
        let renamer = Renamer::new(make_config());
        let mut item = with_scraped_episode(make_tv_item("9号秘事", 1, 2), "9号秘事", 1, 2);
        item.path = std::path::PathBuf::from("/tmp/9号秘事/S01/9号秘事 - S01E02 - Pilot.mp4");

        let plans = renamer.plan(&[item]);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].old_path, plans[0].new_path);
        assert_eq!(plans[0].directory_plans.len(), 1);
        assert_eq!(
            plans[0].directory_plans[0].new_path,
            std::path::PathBuf::from("/tmp/9号秘事/Season 01")
        );
    }

    #[test]
    fn test_movie_directory_plan_skips_tv_extras_folder() {
        let root = tempfile::tempdir().unwrap();
        let show_dir = root.path().join("9号秘事");
        let extras_dir = show_dir.join("万圣节特别篇");
        std::fs::create_dir_all(show_dir.join("Season 01")).unwrap();
        std::fs::create_dir_all(&extras_dir).unwrap();

        let renamer = Renamer::new(make_config());
        let mut item = with_scraped_title(make_movie_item("noise", None), "九号秘事特辑", None);
        item.path = extras_dir.join("noise.mp4");

        let plans = renamer.plan(&[item]);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].directory_plans.is_empty());
    }
}
