use crate::models::media::{ScrapeResult, ScrapeSource};
use std::path::Path;

/// Read metadata from local .nfo file (Kodi/Emby format)
pub fn read_nfo(nfo_path: &Path) -> Option<ScrapeResult> {
    let content = std::fs::read_to_string(nfo_path).ok()?;
    let mut result = ScrapeResult {
        source: ScrapeSource::LocalNfo,
        title: String::new(),
        title_original: None,
        year: None,
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
        tmdb_id: None,
        musicbrainz_id: None,
        openlibrary_id: None,
    };

    // Simple XML-ish parsing (NFO is often malformed XML)
    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = extract_tag(line, "title") {
            result.title = val;
        } else if let Some(val) = extract_tag(line, "originaltitle") {
            result.title_original = Some(val);
        } else if let Some(val) = extract_tag(line, "year") {
            result.year = val.parse().ok();
        } else if let Some(val) = extract_tag(line, "plot") {
            result.overview = Some(val);
        } else if let Some(val) = extract_tag(line, "rating") {
            result.rating = val.parse().ok();
        } else if let Some(val) = extract_tag(line, "season") {
            result.season_number = val.parse().ok();
        } else if let Some(val) = extract_tag(line, "episode") {
            result.episode_number = val.parse().ok();
        } else if let Some(val) = extract_tag(line, "episodename") {
            result.episode_name = Some(val);
        } else if extract_tag(line, "title").is_some() && result.episode_number.is_some() && result.episode_name.is_none() {
            result.episode_name = extract_tag(line, "title");
        } else if let Some(val) = extract_tag(line, "thumb") {
            if result.poster_url.is_none() {
                result.poster_url = Some(val);
            }
        } else if let Some(val) = extract_tag(line, "fanart") {
            result.fanart_url = Some(val);
        } else if let Some(val) = extract_tag(line, "artist") {
            result.artist = Some(val);
        } else if let Some(val) = extract_tag(line, "album") {
            result.album = Some(val);
        } else if let Some(val) = extract_tag(line, "author") {
            result.author = Some(val);
        } else if let Some(val) = extract_tag(line, "uniqueid") {
            // Could be tmdb_id etc.
            if result.tmdb_id.is_none() {
                result.tmdb_id = val.parse().ok();
            }
        }
    }

    if result.title.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Find .nfo file next to a media file
pub fn find_nfo(media_path: &Path) -> Option<std::path::PathBuf> {
    let dir = media_path.parent()?;
    let stem = media_path.file_stem()?.to_string_lossy();
    let nfo = dir.join(format!("{stem}.nfo"));
    if nfo.exists() {
        return Some(nfo);
    }
    // Also check tvshow.nfo in the same directory
    let tvshow_nfo = dir.join("tvshow.nfo");
    if tvshow_nfo.exists() {
        return Some(tvshow_nfo);
    }
    None
}

fn extract_tag(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    if line.starts_with(&open) && line.ends_with(&close) {
        let start = open.len();
        let end = line.len() - close.len();
        if start < end {
            Some(line[start..end].to_string())
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_tag() {
        assert_eq!(extract_tag("<title>Inception</title>", "title"), Some("Inception".into()));
        assert_eq!(extract_tag("<year>2010</year>", "year"), Some("2010".into()));
        assert_eq!(extract_tag("<empty></empty>", "empty"), None);
        assert_eq!(extract_tag("no tag here", "title"), None);
    }

    #[test]
    fn test_read_nfo_movie() {
        let dir = tempfile::tempdir().unwrap();
        let nfo_path = dir.path().join("movie.nfo");
        let mut f = std::fs::File::create(&nfo_path).unwrap();
        writeln!(f, "<?xml version=\"1.0\"?>").unwrap();
        writeln!(f, "<movie>").unwrap();
        writeln!(f, "  <title>Inception</title>").unwrap();
        writeln!(f, "  <year>2010</year>").unwrap();
        writeln!(f, "  <rating>8.8</rating>").unwrap();
        writeln!(f, "</movie>").unwrap();

        let result = read_nfo(&nfo_path).unwrap();
        assert_eq!(result.title, "Inception");
        assert_eq!(result.year, Some(2010));
        assert_eq!(result.rating, Some(8.8));
        assert_eq!(result.source, ScrapeSource::LocalNfo);
    }

    #[test]
    fn test_read_nfo_tv() {
        let dir = tempfile::tempdir().unwrap();
        let nfo_path = dir.path().join("episode.nfo");
        let mut f = std::fs::File::create(&nfo_path).unwrap();
        writeln!(f, "<episodedetails>").unwrap();
        writeln!(f, "  <title>Pilot</title>").unwrap();
        writeln!(f, "  <season>1</season>").unwrap();
        writeln!(f, "  <episode>1</episode>").unwrap();
        writeln!(f, "</episodedetails>").unwrap();

        let result = read_nfo(&nfo_path).unwrap();
        assert_eq!(result.title, "Pilot");
        assert_eq!(result.season_number, Some(1));
        assert_eq!(result.episode_number, Some(1));
    }

    #[test]
    fn test_read_nfo_empty_title() {
        let dir = tempfile::tempdir().unwrap();
        let nfo_path = dir.path().join("empty.nfo");
        let mut f = std::fs::File::create(&nfo_path).unwrap();
        writeln!(f, "<movie><year>2020</year></movie>").unwrap();

        assert!(read_nfo(&nfo_path).is_none());
    }

    #[test]
    fn test_find_nfo() {
        let dir = tempfile::tempdir().unwrap();
        let media_path = dir.path().join("movie.mp4");
        let nfo_path = dir.path().join("movie.nfo");
        std::fs::File::create(&media_path).unwrap();
        std::fs::File::create(&nfo_path).unwrap();

        let found = find_nfo(&media_path).unwrap();
        assert_eq!(found, nfo_path);
    }

    #[test]
    fn test_find_nfo_tvshow() {
        let dir = tempfile::tempdir().unwrap();
        let media_path = dir.path().join("s01e01.mp4");
        let nfo_path = dir.path().join("tvshow.nfo");
        std::fs::File::create(&media_path).unwrap();
        std::fs::File::create(&nfo_path).unwrap();

        let found = find_nfo(&media_path).unwrap();
        assert_eq!(found, nfo_path);
    }
}
