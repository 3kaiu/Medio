use crate::models::media::{ScrapeResult, ScrapeSource};
use std::path::Path;

/// Read metadata from local .nfo file (Kodi/Emby format)
pub fn read_nfo(nfo_path: &Path) -> Option<ScrapeResult> {
    let content = std::fs::read_to_string(nfo_path).ok()?;
    let mut result = ScrapeResult::empty(ScrapeSource::LocalNfo, "").with_confidence(0.98);
    result.push_evidence(format!("loaded local NFO {}", nfo_path.display()));

    let is_episode = content.contains("<episodedetails");
    let title = extract_first_tag(&content, "title");
    let show_title = extract_first_tag(&content, "showtitle");

    if is_episode {
        if let Some(show_title) = show_title.or_else(|| title.clone()) {
            result.title = show_title;
        }
        result.episode_name = extract_first_tag(&content, "episodename").or(title);
    } else if let Some(title) = title {
        result.title = title;
    }

    result.title_original = extract_first_tag(&content, "originaltitle");
    result.year = extract_first_tag(&content, "year").and_then(|value| value.parse().ok());
    result.overview = extract_first_tag(&content, "plot");
    result.rating = extract_first_tag(&content, "rating").and_then(|value| value.parse().ok());
    result.season_number =
        extract_first_tag(&content, "season").and_then(|value| value.parse().ok());
    result.episode_number =
        extract_first_tag(&content, "episode").and_then(|value| value.parse().ok());
    result.poster_url = extract_first_tag(&content, "thumb");
    result.fanart_url = extract_first_tag(&content, "fanart")
        .and_then(|value| extract_first_tag(&value, "thumb").or(Some(value)));
    result.artist = extract_first_tag(&content, "artist");
    result.album = extract_first_tag(&content, "album");
    result.author = extract_first_tag(&content, "author");

    for unique_id in extract_tag_blocks(&content, "uniqueid") {
        let id_type = extract_attr(&unique_id.open_tag, "type")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let value = unique_id.value.trim();
        if value.is_empty() {
            continue;
        }
        match id_type.as_str() {
            "tmdb" if result.tmdb_id.is_none() => {
                result.tmdb_id = value.parse().ok();
            }
            "musicbrainz" if result.musicbrainz_id.is_none() => {
                result.musicbrainz_id = Some(value.to_string());
            }
            "openlibrary" if result.openlibrary_id.is_none() => {
                result.openlibrary_id = Some(value.to_string());
            }
            _ if result.tmdb_id.is_none() => {
                result.tmdb_id = value.parse().ok();
            }
            _ => {}
        }
    }

    if result.title.is_empty() {
        None
    } else {
        result.push_evidence("accepted local NFO as authoritative metadata");
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

    // For badly named episodes, search ancestor show folders for tvshow.nfo
    let mut current = dir.parent();
    let mut depth = 0;
    while let Some(parent) = current {
        if depth >= 2 {
            break;
        }
        let ancestor_tvshow_nfo = parent.join("tvshow.nfo");
        if ancestor_tvshow_nfo.exists() {
            return Some(ancestor_tvshow_nfo);
        }
        current = parent.parent();
        depth += 1;
    }
    None
}

#[derive(Debug, Clone)]
struct TagBlock {
    open_tag: String,
    value: String,
}

fn extract_first_tag(content: &str, tag: &str) -> Option<String> {
    extract_tag_blocks(content, tag)
        .into_iter()
        .map(|block| block.value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn extract_tag_blocks(content: &str, tag: &str) -> Vec<TagBlock> {
    let mut blocks = Vec::new();
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut cursor = 0;

    while let Some(open_rel) = content[cursor..].find(&open_prefix) {
        let open_start = cursor + open_rel;
        let tag_end = open_start + open_prefix.len();
        let next_char = content[tag_end..].chars().next();
        if !matches!(
            next_char,
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r')
        ) {
            cursor = tag_end;
            continue;
        }
        let open_end_rel = match content[open_start..].find('>') {
            Some(index) => index,
            None => break,
        };
        let open_end = open_start + open_end_rel;
        let value_start = open_end + 1;
        let close_rel = match content[value_start..].find(&close_tag) {
            Some(index) => index,
            None => break,
        };
        let value_end = value_start + close_rel;
        blocks.push(TagBlock {
            open_tag: content[open_start..=open_end].to_string(),
            value: content[value_start..value_end].to_string(),
        });
        cursor = value_end + close_tag.len();
    }

    blocks
}

fn extract_attr(open_tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = open_tag.find(&needle)? + needle.len();
    let rest = &open_tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_tag() {
        assert_eq!(
            extract_first_tag("<title>Inception</title>", "title"),
            Some("Inception".into())
        );
        assert_eq!(
            extract_first_tag("<year>2010</year>", "year"),
            Some("2010".into())
        );
        assert_eq!(extract_first_tag("<empty></empty>", "empty"), None);
        assert_eq!(extract_first_tag("no tag here", "title"), None);
        assert_eq!(
            extract_first_tag("<uniqueid type=\"tmdb\">24428</uniqueid>", "uniqueid"),
            Some("24428".into())
        );
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
        writeln!(f, "  <showtitle>Lost</showtitle>").unwrap();
        writeln!(f, "  <season>1</season>").unwrap();
        writeln!(f, "  <episode>1</episode>").unwrap();
        writeln!(
            f,
            "  <uniqueid type=\"tmdb\" default=\"true\">4607</uniqueid>"
        )
        .unwrap();
        writeln!(f, "</episodedetails>").unwrap();

        let result = read_nfo(&nfo_path).unwrap();
        assert_eq!(result.title, "Lost");
        assert_eq!(result.season_number, Some(1));
        assert_eq!(result.episode_number, Some(1));
        assert_eq!(result.episode_name.as_deref(), Some("Pilot"));
        assert_eq!(result.tmdb_id, Some(4607));
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

    #[test]
    fn test_find_nfo_tvshow_in_ancestor_show_dir() {
        let dir = tempfile::tempdir().unwrap();
        let show_dir = dir.path().join("Show");
        let season_dir = show_dir.join("Season 01");
        std::fs::create_dir_all(&season_dir).unwrap();
        let media_path = season_dir.join("01.mkv");
        let nfo_path = show_dir.join("tvshow.nfo");
        std::fs::File::create(&media_path).unwrap();
        std::fs::File::create(&nfo_path).unwrap();

        let found = find_nfo(&media_path).unwrap();
        assert_eq!(found, nfo_path);
    }

    #[test]
    fn test_read_nfo_nested_fanart_thumb() {
        let dir = tempfile::tempdir().unwrap();
        let nfo_path = dir.path().join("show.nfo");
        let mut f = std::fs::File::create(&nfo_path).unwrap();
        writeln!(f, "<tvshow>").unwrap();
        writeln!(f, "  <title>Dark</title>").unwrap();
        writeln!(
            f,
            "  <fanart><thumb>https://example.com/fanart.jpg</thumb></fanart>"
        )
        .unwrap();
        writeln!(f, "</tvshow>").unwrap();

        let result = read_nfo(&nfo_path).unwrap();
        assert_eq!(
            result.fanart_url.as_deref(),
            Some("https://example.com/fanart.jpg")
        );
    }
}
