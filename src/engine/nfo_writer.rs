use crate::models::media::{MediaItem, MediaType, ScrapeResult};

pub fn generate(item: &MediaItem) -> Option<String> {
    let scraped = item.scraped.as_ref()?;
    let mut lines = vec!["<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>".into()];

    match item.media_type {
        MediaType::Movie => {
            lines.push("<movie>".into());
            push_text(&mut lines, "title", Some(scraped.title.as_str()));
            push_text(
                &mut lines,
                "originaltitle",
                scraped.title_original.as_deref(),
            );
            push_year_fields(&mut lines, scraped.year);
            push_numeric(
                &mut lines,
                "rating",
                scraped.rating.map(|value| format!("{value:.1}")),
            );
            push_text(&mut lines, "plot", scraped.overview.as_deref());
            push_runtime(item, &mut lines);
            push_text(&mut lines, "thumb", scraped.poster_url.as_deref());
            push_fanart(&mut lines, scraped.fanart_url.as_deref());
            push_unique_ids(&mut lines, scraped);
            lines.push("</movie>".into());
        }
        MediaType::TvShow => {
            if scraped.episode_number.is_some() {
                lines.push("<episodedetails>".into());
                push_text(
                    &mut lines,
                    "title",
                    scraped
                        .episode_name
                        .as_deref()
                        .or(Some(scraped.title.as_str())),
                );
                push_text(&mut lines, "showtitle", Some(scraped.title.as_str()));
                push_text(
                    &mut lines,
                    "originaltitle",
                    scraped.title_original.as_deref(),
                );
                push_numeric(
                    &mut lines,
                    "season",
                    scraped.season_number.map(|value| value.to_string()),
                );
                push_numeric(
                    &mut lines,
                    "episode",
                    scraped.episode_number.map(|value| value.to_string()),
                );
                push_year_fields(&mut lines, scraped.year);
                push_text(&mut lines, "plot", scraped.overview.as_deref());
                push_numeric(
                    &mut lines,
                    "rating",
                    scraped.rating.map(|value| format!("{value:.1}")),
                );
                push_runtime(item, &mut lines);
                push_text(&mut lines, "thumb", scraped.poster_url.as_deref());
                push_unique_ids(&mut lines, scraped);
                lines.push("</episodedetails>".into());
            } else {
                lines.push("<tvshow>".into());
                push_text(&mut lines, "title", Some(scraped.title.as_str()));
                push_text(
                    &mut lines,
                    "originaltitle",
                    scraped.title_original.as_deref(),
                );
                push_year_fields(&mut lines, scraped.year);
                push_text(&mut lines, "plot", scraped.overview.as_deref());
                push_numeric(
                    &mut lines,
                    "rating",
                    scraped.rating.map(|value| format!("{value:.1}")),
                );
                push_runtime(item, &mut lines);
                push_text(&mut lines, "thumb", scraped.poster_url.as_deref());
                push_fanart(&mut lines, scraped.fanart_url.as_deref());
                push_unique_ids(&mut lines, scraped);
                lines.push("</tvshow>".into());
            }
        }
        MediaType::Music => {
            lines.push("<music>".into());
            push_text(&mut lines, "title", Some(scraped.title.as_str()));
            push_text(&mut lines, "artist", scraped.artist.as_deref());
            push_text(&mut lines, "album", scraped.album.as_deref());
            push_unique_ids(&mut lines, scraped);
            lines.push("</music>".into());
        }
        _ => return None,
    }

    Some(lines.join("\n"))
}

fn push_text(lines: &mut Vec<String>, tag: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("  <{tag}>{}</{tag}>", escape_xml(value)));
    }
}

fn push_numeric(lines: &mut Vec<String>, tag: &str, value: Option<String>) {
    if let Some(value) = value {
        lines.push(format!("  <{tag}>{}</{tag}>", escape_xml(value.trim())));
    }
}

fn push_year_fields(lines: &mut Vec<String>, year: Option<u16>) {
    if let Some(year) = year {
        lines.push(format!("  <year>{year}</year>"));
        lines.push(format!("  <premiered>{year}-01-01</premiered>"));
    }
}

fn push_runtime(item: &MediaItem, lines: &mut Vec<String>) {
    let runtime_secs = item
        .quality
        .as_ref()
        .and_then(|quality| quality.duration_secs)
        .or_else(|| {
            item.content_evidence
                .as_ref()
                .and_then(|content| content.runtime_secs)
        });
    if let Some(runtime_secs) = runtime_secs {
        let runtime_mins = (runtime_secs / 60).max(1);
        lines.push(format!("  <runtime>{runtime_mins}</runtime>"));
    }
}

fn push_fanart(lines: &mut Vec<String>, fanart_url: Option<&str>) {
    if let Some(fanart_url) = fanart_url.map(str::trim).filter(|value| !value.is_empty()) {
        let escaped = escape_xml(fanart_url);
        lines.push("  <fanart>".into());
        lines.push(format!("    <thumb>{escaped}</thumb>"));
        lines.push("  </fanart>".into());
    }
}

fn push_unique_ids(lines: &mut Vec<String>, scraped: &ScrapeResult) {
    if let Some(id) = scraped.tmdb_id {
        lines.push(format!(
            "  <uniqueid type=\"tmdb\" default=\"true\">{id}</uniqueid>"
        ));
    }
    if let Some(id) = scraped.musicbrainz_id.as_deref() {
        lines.push(format!(
            "  <uniqueid type=\"musicbrainz\">{}</uniqueid>",
            escape_xml(id)
        ));
    }
    if let Some(id) = scraped.openlibrary_id.as_deref() {
        lines.push(format!(
            "  <uniqueid type=\"openlibrary\">{}</uniqueid>",
            escape_xml(id)
        ));
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::media::{
        ConfirmationState, ContentEvidence, IdentityResolution, MediaItem, MediaType, ProbeSource,
        QualityInfo, ScrapeResult, ScrapeSource,
    };
    use std::path::PathBuf;

    fn make_item(media_type: MediaType) -> MediaItem {
        MediaItem {
            id: 7,
            path: PathBuf::from("/tmp/test.mkv"),
            file_size: 2048,
            media_type,
            extension: "mkv".into(),
            parsed: None,
            quality: Some(QualityInfo {
                width: Some(3840),
                height: Some(2160),
                resolution_label: "2160p".into(),
                video_codec: Some("hevc".into()),
                video_bitrate: Some(12_000_000),
                audio_codec: Some("truehd".into()),
                audio_bitrate: Some(768_000),
                duration_secs: Some(5_400),
                quality_score: 95.0,
                probe_source: ProbeSource::Ffprobe,
            }),
            scraped: None,
            content_evidence: Some(ContentEvidence {
                runtime_secs: Some(5_400),
                ..Default::default()
            }),
            identity_resolution: Some(IdentityResolution {
                confirmation_state: ConfirmationState::Confirmed,
                best: None,
                candidates: Vec::new(),
                evidence_refs: vec!["fixture".into()],
                risk_flags: Vec::new(),
            }),
            hash: None,
            rename_plan: None,
        }
    }

    #[test]
    fn writes_movie_nfo_with_canonical_ids() {
        let mut item = make_item(MediaType::Movie);
        item.scraped = Some({
            let mut scraped =
                ScrapeResult::empty(ScrapeSource::Tmdb, "Dune & Part <One>").with_confidence(0.95);
            scraped.title_original = Some("Dune".into());
            scraped.year = Some(2021);
            scraped.overview = Some("Spice & prophecy".into());
            scraped.rating = Some(8.3);
            scraped.poster_url = Some("https://example.com/poster.jpg".into());
            scraped.fanart_url = Some("https://example.com/fanart.jpg".into());
            scraped.tmdb_id = Some(438631);
            scraped
        });

        let nfo = generate(&item).unwrap();
        assert!(nfo.contains("<movie>"));
        assert!(nfo.contains("<title>Dune &amp; Part &lt;One&gt;</title>"));
        assert!(nfo.contains("<originaltitle>Dune</originaltitle>"));
        assert!(nfo.contains("<premiered>2021-01-01</premiered>"));
        assert!(nfo.contains("<runtime>90</runtime>"));
        assert!(nfo.contains("<uniqueid type=\"tmdb\" default=\"true\">438631</uniqueid>"));
    }

    #[test]
    fn writes_episode_nfo_with_showtitle() {
        let mut item = make_item(MediaType::TvShow);
        item.scraped = Some({
            let mut scraped =
                ScrapeResult::empty(ScrapeSource::Tmdb, "Severance").with_confidence(0.96);
            scraped.title_original = Some("Severance".into());
            scraped.season_number = Some(2);
            scraped.episode_number = Some(3);
            scraped.episode_name = Some("Who Is Alive?".into());
            scraped.tmdb_id = Some(95396);
            scraped
        });

        let nfo = generate(&item).unwrap();
        assert!(nfo.contains("<episodedetails>"));
        assert!(nfo.contains("<title>Who Is Alive?</title>"));
        assert!(nfo.contains("<showtitle>Severance</showtitle>"));
        assert!(nfo.contains("<season>2</season>"));
        assert!(nfo.contains("<episode>3</episode>"));
    }
}
