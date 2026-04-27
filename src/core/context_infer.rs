use crate::models::media::{MediaItem, MediaType, ParseSource, ParsedInfo};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub struct ContextInfer;

impl ContextInfer {
    /// Collect up to `max` parent directories of a path
    pub fn collect_parent_dirs(path: &Path, max: usize) -> Vec<&Path> {
        let mut dirs = Vec::new();
        let mut current = path.parent();
        while let Some(dir) = current {
            if dirs.len() >= max {
                break;
            }
            dirs.push(dir);
            current = dir.parent();
        }
        dirs
    }

    /// Infer missing year/season from parent directory names
    pub fn infer(parsed: &ParsedInfo, parent_dirs: &[&Path]) -> ParsedInfo {
        let mut result = parsed.clone();

        fill_episode_markers_from_title(&mut result);

        // Infer season from parent dir (e.g., "Season 1", "S01")
        if result.season.is_none() {
            static RE_SEASON: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"(?i)Season\s*(\d{1,2})|S(\d{1,2})\s*$").unwrap());
            for dir in parent_dirs.iter().take(3) {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(caps) = RE_SEASON.captures(&name) {
                    let s = caps
                        .get(1)
                        .or(caps.get(2))
                        .and_then(|m| m.as_str().parse().ok());
                    if let Some(season) = s {
                        result.season = Some(season);
                        if result.parse_source == ParseSource::Regex {
                            // Keep original source if regex already found something
                        } else {
                            result.parse_source = ParseSource::Context;
                        }
                        break;
                    }
                }
            }
        }

        // Infer year from parent dir (e.g., "2023", "(2023)")
        if result.year.is_none() {
            static RE_YEAR: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"[\[(\s](\d{4})[\])\s]").unwrap());
            for dir in parent_dirs.iter().take(3) {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(caps) = RE_YEAR.captures(&name) {
                    if let Some(y) = caps.get(1).and_then(|m| m.as_str().parse().ok()) {
                        if y >= 1900 && y <= 2030 {
                            result.year = Some(y);
                            if result.parse_source != ParseSource::Regex {
                                result.parse_source = ParseSource::Context;
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Infer title from parent dir if raw_title is empty or just an episode marker.
        if should_infer_title_from_parent(&result.raw_title) {
            for dir in parent_dirs.iter().take(2) {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if looks_like_real_title_dir(&name) {
                    result.raw_title = name;
                    result.parse_source = ParseSource::Context;
                    break;
                }
            }
        }

        result
    }

    pub fn enrich_item(item: &mut MediaItem) {
        if let Some(parsed) = &item.parsed {
            let parent_dirs = Self::collect_parent_dirs(&item.path, 3);
            let inferred = Self::infer(parsed, &parent_dirs);
            if inferred.season.is_some() || inferred.episode.is_some() {
                item.media_type = MediaType::TvShow;
            }
            item.parsed = Some(inferred);
        }
    }
}

fn should_infer_title_from_parent(title: &str) -> bool {
    let title = title.trim();
    if title.is_empty() {
        return true;
    }

    static RE_PLACEHOLDER_EXACT: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?ix)
            ^
            (?:
                s\d{1,2}(?:e\d{1,3})? |
                e\d{1,3} |
                ep?\d{1,3} |
                \d{1,3}
            )
            (?:\s*\(\d+\))?
            $
        ",
        )
        .unwrap()
    });

    static RE_PLACEHOLDER_PREFIX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?ix)
            ^
            (?:
                s\d{1,2}e\d{1,3} |
                e\d{1,3} |
                ep?\d{1,3}
            )
            (?:
                [\s._-]+.*
            )?
            $
        ",
        )
        .unwrap()
    });

    RE_PLACEHOLDER_EXACT.is_match(title) || RE_PLACEHOLDER_PREFIX.is_match(title)
}

fn fill_episode_markers_from_title(parsed: &mut ParsedInfo) {
    let title = parsed.raw_title.trim();

    static RE_SEASON_EPISODE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^S(?P<season>\d{1,2})E(?P<episode>\d{1,3})(?:[\s._-].*|\s*\(\d+\))?$")
            .unwrap()
    });
    static RE_EPISODE_ONLY: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)^(?:E|EP)?(?P<episode>\d{1,3})(?:[\s._-].*|\s*\(\d+\))?$").unwrap()
    });

    if parsed.episode.is_none() {
        if let Some(caps) = RE_SEASON_EPISODE.captures(title) {
            parsed.season = parsed.season.or_else(|| {
                caps.name("season")
                    .and_then(|m| m.as_str().parse::<u32>().ok())
            });
            parsed.episode = caps
                .name("episode")
                .and_then(|m| m.as_str().parse::<u32>().ok());
        } else if let Some(caps) = RE_EPISODE_ONLY.captures(title) {
            parsed.episode = caps
                .name("episode")
                .and_then(|m| m.as_str().parse::<u32>().ok());
        }
    }
}

fn looks_like_real_title_dir(name: &str) -> bool {
    if name.starts_with('.') {
        return false;
    }

    static RE_JUNK: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?ix)
            ^
            (?:
                season\s*\d{1,2} |
                s\d{1,2} |
                e\d{1,3} |
                ep?\d{1,3} |
                complete |
                \d{4} |
                \d{1,3}
            )
            $
        ",
        )
        .unwrap()
    });

    !RE_JUNK.is_match(name.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::media::ParseSource;

    fn make_parsed(title: &str) -> ParsedInfo {
        ParsedInfo {
            raw_title: title.into(),
            year: None,
            season: None,
            episode: None,
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
        }
    }

    #[test]
    fn test_infer_season_from_dir() {
        let parsed = make_parsed("Episode 1");
        let dir = Path::new("/media/Show/Season 1");
        let result = ContextInfer::infer(&parsed, &[dir]);
        assert_eq!(result.season, Some(1));
    }

    #[test]
    fn test_infer_season_s01() {
        let parsed = make_parsed("Episode 1");
        let dir = Path::new("/media/Show/S01");
        let result = ContextInfer::infer(&parsed, &[dir]);
        assert_eq!(result.season, Some(1));
    }

    #[test]
    fn test_infer_year_from_dir() {
        let parsed = make_parsed("Movie");
        let dir = Path::new("/media/Movies/Movie (2023)");
        let result = ContextInfer::infer(&parsed, &[dir]);
        assert_eq!(result.year, Some(2023));
    }

    #[test]
    fn test_no_infer_when_already_set() {
        let mut parsed = make_parsed("Show");
        parsed.season = Some(2);
        let dir = Path::new("/media/Show/Season 1");
        let result = ContextInfer::infer(&parsed, &[dir]);
        assert_eq!(result.season, Some(2)); // kept original
    }

    #[test]
    fn test_infer_title_from_dir() {
        let parsed = make_parsed("");
        let dir = Path::new("/media/Breaking Bad");
        let result = ContextInfer::infer(&parsed, &[dir]);
        assert_eq!(result.raw_title, "Breaking Bad");
    }

    #[test]
    fn test_infer_title_for_episode_only_name() {
        let parsed = make_parsed("S07E02");
        let season_dir = Path::new("/media/9号秘事/S07");
        let show_dir = Path::new("/media/9号秘事");
        let result = ContextInfer::infer(&parsed, &[season_dir, show_dir]);
        assert_eq!(result.raw_title, "9号秘事");
    }

    #[test]
    fn test_infer_title_for_numeric_filename() {
        let parsed = make_parsed("01");
        let show_dir = Path::new("/media/财阀家的小儿子");
        let result = ContextInfer::infer(&parsed, &[show_dir]);
        assert_eq!(result.raw_title, "财阀家的小儿子");
    }

    #[test]
    fn test_extract_episode_from_episode_only_title() {
        let parsed = make_parsed("01");
        let show_dir = Path::new("/media/财阀家的小儿子");
        let result = ContextInfer::infer(&parsed, &[show_dir]);
        assert_eq!(result.episode, Some(1));
    }

    #[test]
    fn test_extract_season_and_episode_from_title() {
        let parsed = make_parsed("S07E02");
        let season_dir = Path::new("/media/9号秘事/S07");
        let show_dir = Path::new("/media/9号秘事");
        let result = ContextInfer::infer(&parsed, &[season_dir, show_dir]);
        assert_eq!(result.season, Some(7));
        assert_eq!(result.episode, Some(2));
    }

    #[test]
    fn test_infer_title_when_episode_marker_has_suffix_noise() {
        let parsed = make_parsed("S05E09. 中英字幕");
        let season_dir = Path::new("/media/黄石 1-5季/S05");
        let show_dir = Path::new("/media/黄石 1-5季");
        let result = ContextInfer::infer(&parsed, &[season_dir, show_dir]);
        assert_eq!(result.raw_title, "黄石 1-5季");
        assert_eq!(result.season, Some(5));
        assert_eq!(result.episode, Some(9));
    }
}
