pub mod tmdb;
pub mod musicbrainz;
pub mod openlibrary;
pub mod local;
pub mod image_scraper;

use crate::ai::embedding::EmbeddingClient;
use crate::ai::openai_compat::OpenAiCompat;
use crate::core::config::AppConfig;
use crate::db::cache::Cache;
use crate::models::media::{MediaItem, MediaType, ParsedInfo, ScrapeResult, ScrapeSource};
use futures::stream::{self, StreamExt};
use musicbrainz::MusicBrainzScraper;
use openlibrary::OpenLibraryScraper;
use tmdb::TmdbScraper;

pub async fn populate_scrape_results(items: &mut [MediaItem], config: &AppConfig) {
    let tmdb = TmdbScraper::new(&config.api);
    let mb = MusicBrainzScraper::new(&config.api);
    let ol = OpenLibraryScraper::new();
    let concurrency = config.ai.concurrency.max(1);
    let fallback_chain = config.scrape.fallback_chain.clone();
    let chinese_priority = config.scrape.chinese_title_priority;
    let ai_client = if config.ai.enabled {
        Some(OpenAiCompat::from_config(&config.ai))
    } else {
        None
    };
    let embedding_client = EmbeddingClient::from_config(&config.ai);

    // Open cache
    let cache_path = config.cache_path();
    let cache = Cache::open(&cache_path).ok();
    if let Some(ref cache) = cache {
        let _ = cache.cleanup(config.cache.ttl_days);
    }

    // Preload cache hits
    if let Some(ref cache) = cache {
        for item in items.iter_mut() {
            if item.scraped.is_none() {
                if let Some(parsed) = &item.parsed {
                    let cache_key = format!("{}:{:?}", parsed.raw_title, item.media_type);
                    if let Some(cached) = cache.get_scrape(&cache_key) {
                        item.scraped = Some(cached);
                    }
                }
            }
        }
    }

    // Resolve remaining items through configured fallback chain
    let indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.scraped.is_none())
        .map(|(i, _)| i)
        .collect();

    let results: Vec<(usize, Option<ScrapeResult>)> = stream::iter(indices)
        .map(|idx| {
            let item = &items[idx];
            let media_type = item.media_type;
            let parsed = item.parsed.clone();
            let path = item.path.clone();
            let tmdb = tmdb.clone();
            let mb = mb.clone();
            let ol = ol.clone();
            let fallback_chain = fallback_chain.clone();
            let chinese_priority = chinese_priority;
            let ai_client = ai_client.as_ref().map(|_| OpenAiCompat::from_config(&config.ai));
            let embedding_client = embedding_client.clone();
            async move {
                let result = scrape_with_fallback(
                    &path,
                    &parsed,
                    media_type,
                    &tmdb,
                    &mb,
                    &ol,
                    ai_client.as_ref(),
                    &embedding_client,
                    &fallback_chain,
                    chinese_priority,
                )
                .await;
                (idx, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    for (idx, result) in results {
        if let Some(result) = result {
            // Write to cache
            if let Some(ref cache) = cache {
                if let Some(parsed) = &items[idx].parsed {
                    let cache_key = format!("{}:{:?}", parsed.raw_title, items[idx].media_type);
                    let _ = cache.set_scrape(&cache_key, &result);
                }
            }
            items[idx].scraped = Some(result);
        }
    }

    // Flush cache
    if let Some(ref cache) = cache {
        let _ = cache.flush();
    }
}

async fn scrape_with_fallback(
    path: &std::path::Path,
    parsed: &Option<ParsedInfo>,
    media_type: MediaType,
    tmdb: &TmdbScraper,
    mb: &MusicBrainzScraper,
    ol: &OpenLibraryScraper,
    ai_client: Option<&OpenAiCompat>,
    embedding_client: &EmbeddingClient,
    fallback_chain: &[String],
    chinese_priority: bool,
) -> Option<ScrapeResult> {
    for source in fallback_chain {
        let result = match source.trim().to_ascii_lowercase().as_str() {
            "local" => {
                local::find_nfo(path).and_then(|nfo_path| local::read_nfo(&nfo_path))
            }
            "tmdb" => {
                if matches!(media_type, MediaType::Movie | MediaType::TvShow) {
                    if let Some(parsed) = parsed.as_ref() {
                        let lang = if chinese_priority { Some("zh-CN") } else { None };
                        // Fetch multiple candidates for embedding reranking
                        let candidates = match media_type {
                            MediaType::Movie => tmdb.search_movie_candidates(&parsed.raw_title, parsed.year, lang, 5).await.ok().unwrap_or_default(),
                            MediaType::TvShow => tmdb.search_tv_candidates(&parsed.raw_title, parsed.year, lang, 5).await.ok().unwrap_or_default(),
                            _ => Vec::new(),
                        };
                        if candidates.len() == 1 {
                            Some(candidates.into_iter().next().unwrap())
                        } else if candidates.len() > 1 {
                            // Use embedding reranking if configured
                            if embedding_client.is_configured() {
                                let query = format!("{} {}", parsed.raw_title, parsed.year.map(|y| y.to_string()).unwrap_or_default());
                                if let Ok(ranked) = embedding_client.rerank(&query, &candidates).await {
                                    if let Some((best_idx, _)) = ranked.first() {
                                        candidates.get(*best_idx).cloned()
                                    } else {
                                        candidates.into_iter().next()
                                    }
                                } else {
                                    candidates.into_iter().next()
                                }
                            } else {
                                candidates.into_iter().next()
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "musicbrainz" => {
                if matches!(media_type, MediaType::Music) {
                    if let Some(parsed) = parsed.as_ref() {
                        mb.search_recording(
                            parsed.raw_title.split('.').next().unwrap_or(&parsed.raw_title),
                            &parsed.raw_title,
                        )
                        .await
                        .ok()
                        .flatten()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "openlibrary" | "ol" => {
                if matches!(media_type, MediaType::Novel) {
                    if let Some(parsed) = parsed.as_ref() {
                        ol.search(&parsed.raw_title, None).await.ok().flatten()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "ai" => {
                if let Some(client) = ai_client {
                    let filename = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    client.identify(&filename).await.ok().flatten()
                } else {
                    None
                }
            }
            "guess" => parsed.as_ref().and_then(guess_from_parsed),
            _ => None,
        };

        if result.is_some() {
            return result;
        }
    }

    None
}

fn guess_from_parsed(parsed: &ParsedInfo) -> Option<ScrapeResult> {
    if parsed.raw_title.trim().is_empty() {
        return None;
    }

    Some(ScrapeResult {
        source: ScrapeSource::Guess,
        title: parsed.raw_title.clone(),
        title_original: None,
        year: parsed.year,
        overview: None,
        rating: None,
        season_number: parsed.season,
        episode_number: parsed.episode,
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{AiConfig, ApiConfig};
    use crate::models::media::{ParseSource, ParsedInfo};

    #[test]
    fn test_guess_from_parsed() {
        let parsed = ParsedInfo {
            raw_title: "Inception".into(),
            year: Some(2010),
            season: None,
            episode: None,
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
        };

        let guessed = guess_from_parsed(&parsed).unwrap();
        assert_eq!(guessed.source, ScrapeSource::Guess);
        assert_eq!(guessed.title, "Inception");
        assert_eq!(guessed.year, Some(2010));
    }

    #[test]
    fn test_guess_fallback_chain_returns_guess() {
        let parsed = ParsedInfo {
            raw_title: "Arrival".into(),
            year: Some(2016),
            season: None,
            episode: None,
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
        };
        let tmdb = TmdbScraper::new(&ApiConfig::default());
        let mb = MusicBrainzScraper::new(&ApiConfig::default());
        let ol = OpenLibraryScraper::new();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(scrape_with_fallback(
            std::path::Path::new("/tmp/Arrival.2016.mkv"),
            &Some(parsed),
            MediaType::Movie,
            &tmdb,
            &mb,
            &ol,
            Some(&OpenAiCompat::from_config(&AiConfig::default())),
            &EmbeddingClient::from_config(&AiConfig::default()),
            &["guess".into()],
            false,
        ));

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.source, ScrapeSource::Guess);
        assert_eq!(result.title, "Arrival");
    }
}
