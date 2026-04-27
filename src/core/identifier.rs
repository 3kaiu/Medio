use crate::core::keyword_filter::KeywordFilter;
use crate::models::media::{MediaItem, MediaType, ParsedInfo, ParseSource};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub struct Identifier {
    keyword_filter: KeywordFilter,
}

impl Identifier {
    pub fn new(keyword_filter: KeywordFilter) -> Self {
        Self { keyword_filter }
    }

    /// Parse a single filename
    pub fn parse(&self, filename: &str, media_type: MediaType) -> ParsedInfo {
        let stem = Path::new(filename)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| filename.to_string());

        let cleaned = self.keyword_filter.filter(&stem);

        // Try patterns in priority order
        if let Some(parsed) = self.try_tv_pattern(&cleaned) {
            return parsed;
        }
        if let Some(parsed) = self.try_movie_pattern(&cleaned) {
            return parsed;
        }

        // Fallback: extract what we can, rest is title
        self.fallback_parse(&cleaned, media_type)
    }

    /// Batch parse with rayon (modifies items in place)
    pub fn parse_batch(&self, items: &mut [MediaItem]) {
        use rayon::prelude::*;
        items.par_iter_mut().for_each(|item| {
            let filename = item.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            item.parsed = Some(self.parse(&filename, item.media_type));
        });
    }

    /// TV pattern: S01E02, 1x02, etc.
    fn try_tv_pattern(&self, cleaned: &str) -> Option<ParsedInfo> {
        static RE_SE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)(.+?)[.\s_-]+S(\d{1,2})\s*E(\d{1,3})").unwrap()
        });
        static RE_X: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)(.+?)[.\s_-]+(\d{1,2})x(\d{1,3})").unwrap()
        });

        if let Some(caps) = RE_SE.captures(cleaned) {
            let raw_title = clean_title(caps.get(1)?.as_str());
            let suffix_start = caps.get(2)?.start();
            let media_suffix = extract_media_suffix(cleaned, suffix_start);
            return Some(ParsedInfo {
                raw_title,
                season: Some(caps.get(2)?.as_str().parse().ok()?),
                episode: Some(caps.get(3)?.as_str().parse().ok()?),
                year: extract_year(cleaned),
                resolution: extract_resolution(cleaned),
                codec: extract_codec(cleaned),
                source: extract_source(cleaned),
                release_group: extract_release_group(cleaned),
                media_suffix,
                parse_source: ParseSource::Regex,
            });
        }

        if let Some(caps) = RE_X.captures(cleaned) {
            let raw_title = clean_title(caps.get(1)?.as_str());
            let suffix_start = caps.get(2)?.start();
            let media_suffix = extract_media_suffix(cleaned, suffix_start);
            return Some(ParsedInfo {
                raw_title,
                season: Some(caps.get(2)?.as_str().parse().ok()?),
                episode: Some(caps.get(3)?.as_str().parse().ok()?),
                year: extract_year(cleaned),
                resolution: extract_resolution(cleaned),
                codec: extract_codec(cleaned),
                source: extract_source(cleaned),
                release_group: extract_release_group(cleaned),
                media_suffix,
                parse_source: ParseSource::Regex,
            });
        }

        None
    }

    /// Movie pattern: title + year
    fn try_movie_pattern(&self, cleaned: &str) -> Option<ParsedInfo> {
        static RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)(.+?)[.\s_-]+(\d{4})").unwrap()
        });

        if let Some(caps) = RE.captures(cleaned) {
            let raw_title = clean_title(caps.get(1)?.as_str());
            let year: u16 = caps.get(2)?.as_str().parse().ok()?;
            if year < 1900 || year > 2030 {
                return None;
            }
            let suffix_start = caps.get(2)?.end();
            let media_suffix = extract_media_suffix(cleaned, suffix_start);
            return Some(ParsedInfo {
                raw_title,
                year: Some(year),
                season: None,
                episode: None,
                resolution: extract_resolution(cleaned),
                codec: extract_codec(cleaned),
                source: extract_source(cleaned),
                release_group: extract_release_group(cleaned),
                media_suffix,
                parse_source: ParseSource::Regex,
            });
        }

        None
    }

    /// Fallback: just strip known tags and use remainder as title
    fn fallback_parse(&self, cleaned: &str, _media_type: MediaType) -> ParsedInfo {
        let mut title = cleaned.to_string();

        // Remove known tags from title
        static TAGS: Lazy<Vec<Regex>> = Lazy::new(|| {
            vec![
                Regex::new(r"(?i)\b(1080[pi]|2160[pi]|720[pi]|4[UK])\b").unwrap(),
                Regex::new(r"(?i)\b(H\.?26[45]|HEVC|AV1|x26[45]|x264)\b").unwrap(),
                Regex::new(r"(?i)\b(WEB-?DL|BluRay|Remux|HDTV|CAM|WEBRip|BDRip|BRRip)\b").unwrap(),
                Regex::new(r"(?i)\b(AAC|FLAC|DTS|Atmos|DD5\.?1)\b").unwrap(),
                Regex::new(r"(?i)-[A-Za-z0-9]+$").unwrap(),
            ]
        });
        static COLLAPSE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

        for re in TAGS.iter() {
            title = re.replace_all(&title, " ").to_string();
        }
        title = COLLAPSE.replace_all(&title, " ").trim().to_string();

        let media_suffix = extract_media_suffix(cleaned, 0);

        ParsedInfo {
            raw_title: title,
            year: extract_year(cleaned),
            season: None,
            episode: None,
            resolution: extract_resolution(cleaned),
            codec: extract_codec(cleaned),
            source: extract_source(cleaned),
            release_group: extract_release_group(cleaned),
            media_suffix,
            parse_source: ParseSource::Regex,
        }
    }
}

// --- Helper extraction functions ---

fn clean_title(s: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
    let s = s.trim();
    let s = s.trim_end_matches(|c: char| c == '.' || c == '-' || c == '_' || c == ' ');
    RE.replace_all(s, " ").to_string()
}

fn extract_year(s: &str) -> Option<u16> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[.\s_-](\d{4})[.\s_-]").unwrap());
    RE.captures(s).and_then(|c| c.get(1)).and_then(|m| m.as_str().parse().ok())
}

fn extract_resolution(s: &str) -> Option<String> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(1080[pi]|2160[pi]|720[pi]|4[UK])\b").unwrap()
    });
    RE.captures(s).and_then(|c| c.get(1)).map(|m| m.as_str().to_uppercase())
}

fn extract_codec(s: &str) -> Option<String> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(H\.?26[45]|HEVC|AV1|x26[45]|x264)\b").unwrap()
    });
    RE.captures(s).and_then(|c| c.get(1)).map(|m| {
        let upper = m.as_str().to_uppercase();
        match upper.as_str() {
            "HEVC" => "H.265".into(),
            other => other.into(),
        }
    })
}

fn extract_source(s: &str) -> Option<String> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\b(WEB-?DL|BluRay|Remux|HDTV|CAM|WEBRip|BDRip|BRRip)\b").unwrap()
    });
    RE.captures(s).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

fn extract_release_group(s: &str) -> Option<String> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)-([A-Za-z0-9]+)$").unwrap());
    RE.captures(s).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

fn extract_media_suffix(s: &str, start: usize) -> Option<String> {
    if start >= s.len() {
        return None;
    }
    let suffix_part = &s[start..];
    if suffix_part.trim().is_empty() {
        return None;
    }
    // Collect resolution + source + codec + audio + group
    let mut parts: Vec<String> = Vec::new();
    if let Some(r) = extract_resolution(s) { parts.push(r); }
    if let Some(src) = extract_source(s) { parts.push(src); }
    if let Some(c) = extract_codec(s) { parts.push(c); }
    // Audio
    static AUDIO_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(AAC|FLAC|DTS|Atmos|DD5\.?1)\b").unwrap());
    if let Some(c) = AUDIO_RE.captures(s).and_then(|c| c.get(1)) {
        parts.push(c.as_str().to_string());
    }
    if let Some(g) = extract_release_group(s) { parts.push(g); }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::media::MediaType;

    fn make_identifier() -> Identifier {
        Identifier::new(KeywordFilter::new(vec![]))
    }

    #[test]
    fn test_tv_s01e02() {
        let id = make_identifier();
        let p = id.parse("Breaking.Bad.S01E02.1080p.WEB-DL.x264-CtrlHD.mp4", MediaType::TvShow);
        assert_eq!(p.raw_title, "Breaking.Bad");
        assert_eq!(p.season, Some(1));
        assert_eq!(p.episode, Some(2));
        assert_eq!(p.resolution, Some("1080P".into()));
        assert_eq!(p.codec, Some("X264".into()));
    }

    #[test]
    fn test_tv_1x02() {
        let id = make_identifier();
        let p = id.parse("The.Office.1x02.Pilot.HDTV.mp4", MediaType::TvShow);
        assert_eq!(p.raw_title, "The.Office");
        assert_eq!(p.season, Some(1));
        assert_eq!(p.episode, Some(2));
    }

    #[test]
    fn test_movie_year() {
        let id = make_identifier();
        let p = id.parse("Inception.2010.1080p.BluRay.x264-SPARKS.mkv", MediaType::Movie);
        assert_eq!(p.raw_title, "Inception");
        assert_eq!(p.year, Some(2010));
        assert_eq!(p.resolution, Some("1080P".into()));
    }

    #[test]
    fn test_movie_year_out_of_range() {
        let id = make_identifier();
        let p = id.parse("Something.1800.mp4", MediaType::Movie);
        // 1800 is out of range, should fallback
        assert_ne!(p.year, Some(1800));
    }

    #[test]
    fn test_fallback_simple() {
        let id = make_identifier();
        let p = id.parse("random_file.mp4", MediaType::Movie);
        assert!(!p.raw_title.is_empty());
    }

    #[test]
    fn test_resolution_extraction() {
        let id = make_identifier();
        let p = id.parse("Test.2160p.UHD.mkv", MediaType::Movie);
        assert_eq!(p.resolution, Some("2160P".into()));
    }

    #[test]
    fn test_codec_hevc() {
        let id = make_identifier();
        let p = id.parse("Test.HEVC.1080p.mkv", MediaType::Movie);
        assert_eq!(p.codec, Some("H.265".into()));
    }

    #[test]
    fn test_release_group() {
        let id = make_identifier();
        let p = id.parse("Movie.2020.1080p.WEB-DL.x264-YTS.mp4", MediaType::Movie);
        assert_eq!(p.release_group, Some("YTS".into()));
    }

    #[test]
    fn test_keyword_filter() {
        let id = Identifier::new(KeywordFilter::new(vec!["ColorTV".into()]));
        let p = id.parse("Show.S01E01.1080p.WEB-DL.ColorTV.mp4", MediaType::TvShow);
        assert_eq!(p.raw_title, "Show");
    }
}
