use crate::models::media::{MediaItem, MediaType};

pub fn generate(item: &MediaItem) -> Option<String> {
    let scraped = item.scraped.as_ref()?;
    let mut lines = Vec::new();

    match item.media_type {
        MediaType::Movie => {
            lines.push("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>".into());
            lines.push("<movie>".into());
            lines.push(format!("  <title>{}</title>", scraped.title));
            if let Some(y) = scraped.year {
                lines.push(format!("  <year>{y}</year>"));
            }
            if let Some(r) = scraped.rating {
                lines.push(format!("  <rating>{r:.1}</rating>"));
            }
            if let Some(o) = &scraped.overview {
                lines.push(format!("  <plot>{o}</plot>"));
            }
            if let Some(p) = &scraped.poster_url {
                lines.push(format!("  <thumb>{p}</thumb>"));
            }
            if let Some(f) = &scraped.fanart_url {
                lines.push(format!("  <fanart><thumb>{f}</thumb></fanart>"));
            }
            if let Some(id) = scraped.tmdb_id {
                lines.push(format!("  <uniqueid type=\"tmdb\">{id}</uniqueid>"));
            }
            lines.push("</movie>".into());
        }
        MediaType::TvShow => {
            lines.push("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>".into());
            if scraped.episode_number.is_some() {
                lines.push("<episodedetails>".into());
                lines.push(format!(
                    "  <title>{}</title>",
                    scraped.episode_name.as_deref().unwrap_or("")
                ));
                if let Some(s) = scraped.season_number {
                    lines.push(format!("  <season>{s}</season>"));
                }
                if let Some(e) = scraped.episode_number {
                    lines.push(format!("  <episode>{e}</episode>"));
                }
                if let Some(o) = &scraped.overview {
                    lines.push(format!("  <plot>{o}</plot>"));
                }
                if let Some(r) = scraped.rating {
                    lines.push(format!("  <rating>{r:.1}</rating>"));
                }
                lines.push("</episodedetails>".into());
            } else {
                lines.push("<tvshow>".into());
                lines.push(format!("  <title>{}</title>", scraped.title));
                if let Some(y) = scraped.year {
                    lines.push(format!("  <year>{y}</year>"));
                }
                if let Some(o) = &scraped.overview {
                    lines.push(format!("  <plot>{o}</plot>"));
                }
                if let Some(p) = &scraped.poster_url {
                    lines.push(format!("  <thumb>{p}</thumb>"));
                }
                if let Some(id) = scraped.tmdb_id {
                    lines.push(format!("  <uniqueid type=\"tmdb\">{id}</uniqueid>"));
                }
                lines.push("</tvshow>".into());
            }
        }
        MediaType::Music => {
            lines.push("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>".into());
            lines.push("<music>".into());
            lines.push(format!("  <title>{}</title>", scraped.title));
            if let Some(a) = &scraped.artist {
                lines.push(format!("  <artist>{a}</artist>"));
            }
            if let Some(al) = &scraped.album {
                lines.push(format!("  <album>{al}</album>"));
            }
            lines.push("</music>".into());
        }
        _ => return None,
    }

    Some(lines.join("\n"))
}
