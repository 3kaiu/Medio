use once_cell::sync::Lazy;
use regex::Regex;

#[allow(dead_code)]
pub struct SuffixExtractor;

impl SuffixExtractor {
    /// Extract media_suffix from filename
    /// e.g. "Movie.2023.2160p.WEB-DL.H.265.AAC-ColorTV" → "2160p.WEB-DL.H.265.AAC.ColorTV"
    #[allow(dead_code)]
    pub fn extract(filename: &str) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();

        // Resolution
        static RE_RES: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)\b(1080[pi]|2160[pi]|720[pi]|4[UK])\b").unwrap()
        });
        if let Some(c) = RE_RES.captures(filename).and_then(|c| c.get(1)) {
            parts.push(c.as_str().to_uppercase());
        }

        // Source
        static RE_SRC: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)\b(WEB-?DL|BluRay|Remux|HDTV|CAM|WEBRip|BDRip|BRRip)\b").unwrap()
        });
        if let Some(c) = RE_SRC.captures(filename).and_then(|c| c.get(1)) {
            parts.push(c.as_str().to_string());
        }

        // Video codec
        static RE_CODEC: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)\b(H\.?26[45]|HEVC|AV1|x26[45]|x264)\b").unwrap()
        });
        if let Some(c) = RE_CODEC.captures(filename).and_then(|c| c.get(1)) {
            let s = c.as_str();
            let upper = s.to_uppercase();
            let normalized = match upper.as_str() {
                "HEVC" => "H.265",
                other => other,
            };
            parts.push(normalized.to_string());
        }

        // Audio codec
        static RE_AUDIO: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)\b(AAC|FLAC|DTS|Atmos|DD5\.?1)\b").unwrap()
        });
        if let Some(c) = RE_AUDIO.captures(filename).and_then(|c| c.get(1)) {
            parts.push(c.as_str().to_string());
        }

        // Release group (trailing -XXX)
        static RE_GROUP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)-([A-Za-z0-9]+)\.\w+$").unwrap());
        if let Some(c) = RE_GROUP.captures(filename).and_then(|c| c.get(1)) {
            parts.push(c.as_str().to_string());
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("."))
        }
    }
}
