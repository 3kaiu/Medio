use crate::models::media::{ParsedInfo, ParseSource};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub struct ContextInfer;

impl ContextInfer {
    /// Infer missing year/season from parent directory names
    pub fn infer(parsed: &ParsedInfo, parent_dirs: &[&Path]) -> ParsedInfo {
        let mut result = parsed.clone();

        // Infer season from parent dir (e.g., "Season 1", "S01")
        if result.season.is_none() {
            static RE_SEASON: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r"(?i)Season\s*(\d{1,2})|S(\d{1,2})\s*$").unwrap()
            });
            for dir in parent_dirs.iter().take(3) {
                let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if let Some(caps) = RE_SEASON.captures(&name) {
                    let s = caps.get(1).or(caps.get(2)).and_then(|m| m.as_str().parse().ok());
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
            static RE_YEAR: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r"[\[(\s](\d{4})[\])\s]").unwrap()
            });
            for dir in parent_dirs.iter().take(3) {
                let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
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

        // Infer title from parent dir if raw_title is empty or looks like garbage
        if result.raw_title.trim().is_empty() {
            for dir in parent_dirs.iter().take(2) {
                let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                // Use dir name as title only if it looks like a real title (not Season/numbers)
                static RE_JUNK: Lazy<Regex> = Lazy::new(|| {
                    Regex::new(r"(?i)^(Season|S\d|Complete|\d{4})$").unwrap()
                });
                if !RE_JUNK.is_match(&name) && !name.starts_with('.') {
                    result.raw_title = name;
                    result.parse_source = ParseSource::Context;
                    break;
                }
            }
        }

        result
    }
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
}
