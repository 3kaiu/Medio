/// Keyword filter — strips noise words from filenames before identification
pub struct KeywordFilter {
    keywords: Vec<String>,
}

impl KeywordFilter {
    pub fn new(keywords: Vec<String>) -> Self {
        let mut all = keywords;
        all.extend(Self::builtin_keywords());
        Self { keywords: all }
    }

    /// Filter noise keywords from filename, return cleaned string
    pub fn filter(&self, filename: &str) -> String {
        let mut result = filename.to_string();
        for kw in &self.keywords {
            let pattern = regex::Regex::new(&format!("(?i){}", regex::escape(kw))).unwrap();
            result = pattern.replace_all(&result, " ").to_string();
        }
        // Collapse multiple spaces
        let collapse = regex::Regex::new(r"\s+").unwrap();
        collapse.replace_all(&result, " ").trim().to_string()
    }

    /// Built-in common release group / source tags
    pub fn builtin_keywords() -> Vec<String> {
        vec![
            // Common release groups
            "ColorTV".into(),
            "WEBDL".into(),
            "HDS".into(),
            "FRDS".into(),
            "BeAst".into(),
            "HDCTV".into(),
            "OurTV".into(),
            "AoE".into(),
            // Common source tags (already extracted by identifier, but clean for safety)
            "字幕侠".into(),
            "片源网".into(),
            "人人影视".into(),
            "射手网".into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_keywords_present() {
        let kws = KeywordFilter::builtin_keywords();
        assert!(!kws.is_empty());
        assert!(kws.iter().any(|k| k == "ColorTV"));
    }

    #[test]
    fn test_filter_builtin() {
        let kf = KeywordFilter::new(vec![]);
        let result = kf.filter("Movie.2024.1080p.ColorTV");
        assert!(!result.contains("ColorTV"));
    }

    #[test]
    fn test_filter_custom() {
        let kf = KeywordFilter::new(vec!["MyGroup".into()]);
        let result = kf.filter("Show.S01E01.MyGroup");
        assert!(!result.contains("MyGroup"));
    }

    #[test]
    fn test_filter_cjk() {
        let kf = KeywordFilter::new(vec![]);
        let result = kf.filter("电影.字幕侠.2024");
        assert!(!result.contains("字幕侠"));
    }

    #[test]
    fn test_filter_preserves_content() {
        let kf = KeywordFilter::new(vec![]);
        let result = kf.filter("Breaking.Bad.S01E02");
        assert!(result.contains("Breaking"));
        assert!(result.contains("Bad"));
    }
}
