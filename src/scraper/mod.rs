pub mod image_scraper;
pub mod local;
pub mod musicbrainz;
pub mod openlibrary;
pub mod tmdb;

use crate::ai::embedding::EmbeddingClient;
use crate::ai::openai_compat::OpenAiCompat;
use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::db::cache::Cache;
use crate::media::content_probe::ContentProbe;
use crate::models::media::{
    ConfirmationState, ContentEvidence, IdentityCandidate, IdentityResolution, MediaItem,
    MediaType, ParsedInfo, ScrapeResult, ScrapeSource,
};
use futures::stream::{self, StreamExt};
use musicbrainz::MusicBrainzScraper;
use openlibrary::OpenLibraryScraper;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tmdb::TmdbScraper;

#[derive(Debug, Clone)]
pub struct ScrapeTrace {
    pub item_index: usize,
    pub details: Vec<String>,
}

struct ScrapeResolution {
    result: Option<ScrapeResult>,
    content_evidence: ContentEvidence,
    identity_resolution: IdentityResolution,
    details: Vec<String>,
}

#[derive(Debug, Clone)]
struct CandidateScore {
    index: usize,
    score: f32,
    reasons: Vec<String>,
}

pub async fn populate_scrape_results(
    items: &mut [MediaItem],
    config: &AppConfig,
) -> Vec<ScrapeTrace> {
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
    let mut traces: Vec<ScrapeTrace> = Vec::new();

    if let Some(ref cache) = cache {
        for (idx, item) in items.iter_mut().enumerate() {
            let content_cache_key = content_probe_cache_key(item);
            if item.content_evidence.is_none()
                && let Some(probe) = cache.get_content_probe(&content_cache_key)
            {
                item.content_evidence = Some(probe);
            }
            if item.scraped.is_none()
                && let Some(parsed) = &item.parsed
            {
                let cache_key = scrape_cache_key(parsed, item.media_type);
                if let Some(cached) = cache.get_scrape(&cache_key) {
                    let mut cached = cached;
                    cached.push_evidence("loaded from scrape cache");
                    cached.push_evidence(format!("cache key {cache_key}"));
                    if let Some(content) = item.content_evidence.as_ref() {
                        let mut risk_flags = content.risk_flags.clone();
                        item.identity_resolution = Some(resolve_identity(
                            &cached,
                            &[],
                            item.media_type,
                            content,
                            item.quality
                                .as_ref()
                                .and_then(|quality| quality.duration_secs),
                            &mut risk_flags,
                        ));
                    }
                    item.scraped = Some(cached);
                    traces.push(ScrapeTrace {
                        item_index: idx,
                        details: vec![
                            "cache hit".into(),
                            format!("cache_key: {cache_key}"),
                            format!(
                                "selected source: {:?}",
                                item.scraped
                                    .as_ref()
                                    .map(|s| s.source)
                                    .unwrap_or(ScrapeSource::Guess)
                            ),
                        ],
                    });
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

    let results: Vec<(usize, ScrapeResolution)> = stream::iter(indices)
        .map(|idx| {
            let item = &items[idx];
            let media_type = item.media_type;
            let parsed = item.parsed.clone();
            let path = item.path.clone();
            let quality = item.quality.clone();
            let mut content_evidence = item.content_evidence.clone();
            let tmdb = tmdb.clone();
            let mb = mb.clone();
            let ol = ol.clone();
            let fallback_chain = fallback_chain.clone();
            let ai_client = ai_client.clone();
            let embedding_client = embedding_client.clone();
            async move {
                let request = ScrapeRequest {
                    path: &path,
                    parsed: &parsed,
                    quality: quality.as_ref(),
                    media_type,
                    content_evidence: content_evidence.get_or_insert_with(|| {
                        let temp = MediaItem {
                            id: 0,
                            path: path.clone(),
                            file_size: 0,
                            media_type,
                            extension: path
                                .extension()
                                .map(|e| e.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            parsed: parsed.clone(),
                            quality: quality.clone(),
                            scraped: None,
                            content_evidence: None,
                            identity_resolution: None,
                            hash: None,
                            rename_plan: None,
                        };
                        ContentProbe::probe(&temp)
                    }),
                };
                let context = ScrapeContext {
                    tmdb: &tmdb,
                    mb: &mb,
                    ol: &ol,
                    ai_client: ai_client.as_ref(),
                    embedding_client: &embedding_client,
                    fallback_chain: &fallback_chain,
                    chinese_priority,
                };
                let result = scrape_with_fallback(request, &context).await;
                (idx, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    for (idx, resolution) in results {
        items[idx].content_evidence = Some(resolution.content_evidence.clone());
        items[idx].identity_resolution = Some(resolution.identity_resolution.clone());
        if let Some(result) = resolution.result {
            // Write to cache
            if let Some(ref cache) = cache
                && let Some(parsed) = &items[idx].parsed
            {
                let cache_key = scrape_cache_key(parsed, items[idx].media_type);
                let _ = cache.set_scrape(&cache_key, &result);
            }
            items[idx].scraped = Some(result);
        }
        if let Some(ref cache) = cache {
            let _ = cache.set_content_probe(
                &content_probe_cache_key(&items[idx]),
                items[idx].content_evidence.as_ref().unwrap(),
            );
        }
        traces.push(ScrapeTrace {
            item_index: idx,
            details: resolution.details,
        });
    }

    // Flush cache
    if let Some(ref cache) = cache {
        let _ = cache.flush();
    }

    traces.sort_by_key(|trace| trace.item_index);
    traces
}

struct ScrapeRequest<'a> {
    path: &'a Path,
    parsed: &'a Option<ParsedInfo>,
    quality: Option<&'a crate::models::media::QualityInfo>,
    media_type: MediaType,
    content_evidence: &'a ContentEvidence,
}

struct ScrapeContext<'a> {
    tmdb: &'a TmdbScraper,
    mb: &'a MusicBrainzScraper,
    ol: &'a OpenLibraryScraper,
    ai_client: Option<&'a OpenAiCompat>,
    embedding_client: &'a EmbeddingClient,
    fallback_chain: &'a [String],
    chinese_priority: bool,
}

async fn scrape_with_fallback(
    request: ScrapeRequest<'_>,
    context: &ScrapeContext<'_>,
) -> ScrapeResolution {
    let mut details = Vec::new();
    details.push(format!(
        "content_probe: titles={} subtitles={} seasons={} episodes={}",
        request.content_evidence.title_candidates.len(),
        request.content_evidence.subtitles.len(),
        request.content_evidence.season_hypotheses.len(),
        request.content_evidence.episode_hypotheses.len()
    ));
    let mut identity_candidates = Vec::new();
    let mut risk_flags = request.content_evidence.risk_flags.clone();
    for source in context.fallback_chain {
        let source_name = source.trim().to_ascii_lowercase();
        let result = match source_name.as_str() {
            "local" => {
                let found = local::find_nfo(request.path);
                if let Some(ref nfo_path) = found {
                    details.push(format!("local: found nfo {}", nfo_path.display()));
                } else {
                    details.push("local: no nfo found".into());
                }
                found.and_then(|nfo_path| local::read_nfo(&nfo_path))
            }
            "tmdb" => {
                if matches!(request.media_type, MediaType::Movie | MediaType::TvShow) {
                    if let Some(parsed) = request.parsed.as_ref() {
                        let lang = if context.chinese_priority {
                            Some("zh-CN")
                        } else {
                            None
                        };
                        let queries = contextual_title_queries(
                            request.path,
                            parsed,
                            request.content_evidence,
                        );
                        details.push(format!("tmdb: {} query variants", queries.len()));
                        let candidates = fetch_tmdb_candidates(
                            context.tmdb,
                            request.media_type,
                            &queries,
                            parsed.year,
                            lang,
                            &mut details,
                        )
                        .await;
                        details.push(format!("tmdb: {} candidates", candidates.len()));
                        let season_hint = preferred_season_hint(parsed, request.content_evidence);
                        let episode_hint = preferred_episode_hint(parsed, request.content_evidence);
                        identity_candidates = build_identity_candidates(
                            parsed,
                            request.media_type,
                            &candidates,
                            season_hint,
                            episode_hint,
                            request.content_evidence,
                        );
                        let selected = if candidates.len() == 1 {
                            details.push("tmdb: selected only candidate".into());
                            candidates.into_iter().next().map(|mut candidate| {
                                candidate.push_evidence("selected only TMDB candidate");
                                candidate
                            })
                        } else if candidates.len() > 1 {
                            select_best_candidate(
                                parsed,
                                request.media_type,
                                &candidates,
                                context.embedding_client,
                                &mut details,
                                request.content_evidence,
                            )
                            .await
                        } else {
                            None
                        };

                        if request.media_type == MediaType::TvShow {
                            if let Some(base) = selected {
                                if let (Some(season), Some(episode)) = (season_hint, episode_hint) {
                                    if let Some(tmdb_id) = base.tmdb_id {
                                        if let Ok(Some(ep_result)) = context
                                            .tmdb
                                            .get_episode_with_lang(tmdb_id, season, episode, lang)
                                            .await
                                        {
                                            details.push(format!(
                                                "tmdb: enriched tv episode S{:02}E{:02}",
                                                season, episode
                                            ));
                                            let mut merged = base;
                                            merged.season_number = ep_result.season_number;
                                            merged.episode_number = ep_result.episode_number;
                                            merged.episode_name = ep_result.episode_name;
                                            merged.poster_url =
                                                ep_result.poster_url.or(merged.poster_url);
                                            merged.push_evidence(format!(
                                                "enriched with TMDB episode metadata S{season:02}E{episode:02}"
                                            ));
                                            merged.confidence = merged.confidence.max(0.95);
                                            Some(merged)
                                        } else {
                                            Some(base)
                                        }
                                    } else {
                                        Some(base)
                                    }
                                } else {
                                    Some(base)
                                }
                            } else {
                                None
                            }
                        } else {
                            selected
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "musicbrainz" => {
                if matches!(request.media_type, MediaType::Music) {
                    if let Some(parsed) = request.parsed.as_ref() {
                        let mut result = None;
                        for (artist, title) in music_query_variants(&parsed.raw_title) {
                            result = context
                                .mb
                                .search_recording(&artist, &title)
                                .await
                                .ok()
                                .flatten();
                            details.push(format!(
                                "musicbrainz: tried artist='{}' title='{}' => {}",
                                artist,
                                title,
                                if result.is_some() { "match" } else { "miss" }
                            ));
                            if result.is_some() {
                                break;
                            }
                        }
                        details.push(format!(
                            "musicbrainz: {}",
                            if result.is_some() {
                                "matched"
                            } else {
                                "no match"
                            }
                        ));
                        result
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "openlibrary" | "ol" => {
                if matches!(request.media_type, MediaType::Novel) {
                    if let Some(parsed) = request.parsed.as_ref() {
                        let mut result = None;
                        for title in
                            contextual_title_queries(request.path, parsed, request.content_evidence)
                        {
                            result = context.ol.search(&title, None).await.ok().flatten();
                            details.push(format!(
                                "openlibrary: tried title='{}' => {}",
                                title,
                                if result.is_some() { "match" } else { "miss" }
                            ));
                            if result.is_some() {
                                break;
                            }
                        }
                        details.push(format!(
                            "openlibrary: {}",
                            if result.is_some() {
                                "matched"
                            } else {
                                "no match"
                            }
                        ));
                        result
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "ai" => {
                if let Some(client) = context.ai_client {
                    let filename = request
                        .path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let parent_context = context_parent_labels(request.path);
                    if let Some(result) = client
                        .identify_with_context(&filename, &parent_context, request.parsed.as_ref())
                        .await
                        .ok()
                        .flatten()
                    {
                        details.push("ai: identified candidate".into());
                        // Try to refine title via suggest_title and re-search TMDB
                        if matches!(request.media_type, MediaType::Movie | MediaType::TvShow) {
                            if let Ok(Some(better_title)) =
                                client.suggest_title(&filename, &result.title).await
                            {
                                details
                                    .push(format!("ai: suggested better title {}", better_title));
                                let lang = if context.chinese_priority {
                                    Some("zh-CN")
                                } else {
                                    None
                                };
                                let re_search = match request.media_type {
                                    MediaType::Movie => context
                                        .tmdb
                                        .search_movie_candidates(
                                            &better_title,
                                            result.year,
                                            lang,
                                            1,
                                        )
                                        .await
                                        .ok()
                                        .and_then(|c| c.into_iter().next()),
                                    MediaType::TvShow => context
                                        .tmdb
                                        .search_tv_candidates(&better_title, result.year, lang, 1)
                                        .await
                                        .ok()
                                        .and_then(|c| c.into_iter().next()),
                                    _ => None,
                                };
                                if re_search.is_some() {
                                    details.push("ai: tmdb accepted suggested title".into());
                                    re_search.map(|mut result| {
                                        result.push_evidence(format!(
                                            "AI suggested title '{}' accepted by TMDB",
                                            better_title
                                        ));
                                        result.confidence = result.confidence.max(0.88);
                                        result
                                    })
                                } else {
                                    details.push("ai: keeping direct AI result".into());
                                    let mut result = result;
                                    result.push_evidence(
                                        "kept direct AI identification after TMDB retry miss",
                                    );
                                    Some(result)
                                }
                            } else {
                                details.push("ai: no better title suggestion".into());
                                let mut result = result;
                                result.push_evidence("AI returned direct identification");
                                Some(result)
                            }
                        } else {
                            let mut result = result;
                            result.push_evidence("AI returned direct identification");
                            Some(result)
                        }
                    } else {
                        details.push("ai: no result".into());
                        None
                    }
                } else {
                    details.push("ai: provider disabled or unconfigured".into());
                    None
                }
            }
            "guess" => {
                let result = request.parsed.as_ref().and_then(guess_from_parsed);
                details.push(format!(
                    "guess: {}",
                    if result.is_some() {
                        "used parsed fallback"
                    } else {
                        "no parsed title"
                    }
                ));
                result
            }
            _ => None,
        };

        if let Some(result) = result {
            let mut result = result;
            for detail in &details {
                result.push_evidence(detail.clone());
            }
            details.push(format!("selected source: {:?}", result.source));
            result.push_evidence(format!("selected source {:?}", result.source));
            let identity_resolution = resolve_identity(
                &result,
                &identity_candidates,
                request.media_type,
                request.content_evidence,
                request.quality.and_then(|quality| quality.duration_secs),
                &mut risk_flags,
            );
            return ScrapeResolution {
                result: Some(result),
                content_evidence: request.content_evidence.clone(),
                identity_resolution,
                details,
            };
        }
    }

    details.push("selected source: none".into());
    ScrapeResolution {
        result: None,
        content_evidence: request.content_evidence.clone(),
        identity_resolution: IdentityResolution {
            confirmation_state: ConfirmationState::InsufficientEvidence,
            best: None,
            candidates: identity_candidates,
            evidence_refs: vec!["no scrape source produced a candidate".into()],
            risk_flags,
        },
        details,
    }
}

fn scrape_cache_key(parsed: &ParsedInfo, media_type: MediaType) -> String {
    format!(
        "{:?}:{}:{}:{}:{}",
        media_type,
        parsed.raw_title.trim(),
        parsed.year.map(|y| y.to_string()).unwrap_or_default(),
        parsed.season.map(|s| s.to_string()).unwrap_or_default(),
        parsed.episode.map(|e| e.to_string()).unwrap_or_default()
    )
}

fn content_probe_cache_key(item: &MediaItem) -> String {
    let modified = std::fs::metadata(&item.path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "{}:{}:{}:{}",
        item.path.display(),
        item.file_size,
        modified,
        item.extension
    )
}

fn guess_from_parsed(parsed: &ParsedInfo) -> Option<ScrapeResult> {
    if parsed.raw_title.trim().is_empty() {
        return None;
    }

    let mut result = ScrapeResult::empty(ScrapeSource::Guess, parsed.raw_title.clone())
        .with_confidence((0.30 + parsed.confidence * 0.35).clamp(0.30, 0.62))
        .with_evidence([
            "generated metadata guess from parsed filename".to_string(),
            format!("parse confidence {:.2}", parsed.confidence),
        ]);
    result.year = parsed.year;
    result.season_number = parsed.season;
    result.episode_number = parsed.episode;
    Some(result)
}

async fn fetch_tmdb_candidates(
    tmdb: &TmdbScraper,
    media_type: MediaType,
    queries: &[String],
    year: Option<u16>,
    lang: Option<&str>,
    details: &mut Vec<String>,
) -> Vec<ScrapeResult> {
    let mut merged = Vec::new();
    let mut seen = HashSet::new();

    for query in queries {
        let candidates = match media_type {
            MediaType::Movie => tmdb
                .search_movie_candidates(query, year, lang, 5)
                .await
                .ok()
                .unwrap_or_default(),
            MediaType::TvShow => tmdb
                .search_tv_candidates(query, year, lang, 5)
                .await
                .ok()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        details.push(format!(
            "tmdb: query '{}' returned {} candidate(s)",
            query,
            candidates.len()
        ));

        for mut candidate in candidates {
            let dedupe_key = candidate
                .tmdb_id
                .map(|id| format!("tmdb:{id}"))
                .unwrap_or_else(|| {
                    format!("title:{}:{}", candidate.title, candidate.year.unwrap_or(0))
                });
            if seen.insert(dedupe_key) {
                candidate.push_evidence(format!("retrieved via TMDB query '{}'", query));
                merged.push(candidate);
            }
        }
    }

    merged
}

fn title_query_variants(title: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let raw = title.trim();
    if raw.is_empty() {
        return variants;
    }

    push_unique_variant(&mut variants, raw.to_string());
    push_unique_variant(&mut variants, normalize_title_query(raw));

    let stripped = strip_title_noise(raw);
    if !stripped.is_empty() {
        push_unique_variant(&mut variants, stripped.clone());
        push_unique_variant(&mut variants, normalize_title_query(&stripped));
    }

    let identity = compact_identity_query(raw);
    if !identity.is_empty() {
        push_unique_variant(&mut variants, identity.clone());
        push_unique_variant(&mut variants, normalize_title_query(&identity));
    }

    variants
}

fn contextual_title_queries(
    path: &Path,
    parsed: &ParsedInfo,
    content_evidence: &ContentEvidence,
) -> Vec<String> {
    let mut variants = title_query_variants(&parsed.raw_title);
    for hint in &content_evidence.title_candidates {
        for query in title_query_variants(hint) {
            push_unique_variant(&mut variants, query);
        }
    }
    for hint in path_context_title_hints(path) {
        for query in title_query_variants(&hint) {
            push_unique_variant(&mut variants, query);
        }
    }
    variants
}

fn path_context_title_hints(path: &Path) -> Vec<String> {
    let parents = ContextInfer::collect_parent_dirs(path, 4);
    let mut hints = Vec::new();
    for dir in parents {
        let Some(name) = dir.file_name() else {
            continue;
        };
        let label = normalize_title_query(&name.to_string_lossy());
        if label.is_empty() || is_context_junk(&label) {
            continue;
        }
        push_unique_variant(&mut hints, label);
    }
    hints
}

fn context_parent_labels(path: &Path) -> Vec<String> {
    ContextInfer::collect_parent_dirs(path, 4)
        .into_iter()
        .filter_map(|dir| {
            dir.file_name()
                .map(|name| normalize_title_query(&name.to_string_lossy()))
        })
        .filter(|label| !label.is_empty())
        .collect()
}

fn music_query_variants(raw_title: &str) -> Vec<(String, String)> {
    let mut variants = Vec::new();
    let normalized = normalize_title_query(raw_title);
    let splitters = [" - ", " – ", " — "];

    for splitter in splitters {
        if let Some((artist, title)) = normalized.split_once(splitter) {
            let artist = artist.trim();
            let title = title.trim();
            if !artist.is_empty() && !title.is_empty() {
                variants.push((artist.to_string(), title.to_string()));
            }
        }
    }

    if variants.is_empty() {
        let parts: Vec<_> = normalized.split_whitespace().collect();
        if parts.len() >= 2 {
            variants.push((parts[0].to_string(), parts[1..].join(" ")));
        }
    }

    if variants.is_empty() {
        variants.push((normalized.clone(), normalized));
    }

    variants
}

fn push_unique_variant(variants: &mut Vec<String>, candidate: String) {
    let candidate = candidate.trim().to_string();
    if candidate.is_empty() {
        return;
    }
    if !variants
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&candidate))
    {
        variants.push(candidate);
    }
}

fn normalize_title_query(title: &str) -> String {
    title
        .replace(['.', '_'], " ")
        .replace("  ", " ")
        .trim()
        .to_string()
}

fn strip_title_noise(title: &str) -> String {
    let normalized = normalize_title_query(title);
    let mut out = Vec::new();
    for token in normalized.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "1080p"
                | "720p"
                | "2160p"
                | "4k"
                | "webrip"
                | "web-dl"
                | "bluray"
                | "brrip"
                | "x264"
                | "x265"
                | "h264"
                | "h265"
                | "hevc"
                | "aac"
                | "dts"
                | "hdr"
                | "dv"
        ) {
            continue;
        }
        out.push(token);
    }
    out.join(" ")
}

fn compact_identity_query(title: &str) -> String {
    let stripped = strip_title_noise(title);
    let mut out = Vec::new();

    for token in stripped.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let is_year = token.len() == 4
            && token
                .parse::<u16>()
                .map(|year| (1900..=2099).contains(&year))
                .unwrap_or(false);
        let is_tv_marker = {
            let bytes = lower.as_bytes();
            bytes.len() == 6
                && bytes[0] == b's'
                && bytes[3] == b'e'
                && bytes[1..3].iter().all(|b| b.is_ascii_digit())
                && bytes[4..6].iter().all(|b| b.is_ascii_digit())
        };

        if is_year || is_tv_marker {
            continue;
        }

        out.push(token);
    }

    out.join(" ")
}

fn is_context_junk(label: &str) -> bool {
    let lower = label.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }
    if matches!(
        lower.as_str(),
        "season 01" | "season 1" | "season 02" | "season 2" | "s01" | "s02" | "s1" | "s2"
    ) {
        return true;
    }

    let bytes = lower.as_bytes();
    bytes.len() <= 6
        && bytes.first() == Some(&b's')
        && bytes.iter().skip(1).all(|b| b.is_ascii_digit())
}

async fn select_best_candidate(
    parsed: &ParsedInfo,
    media_type: MediaType,
    candidates: &[ScrapeResult],
    embedding_client: &EmbeddingClient,
    details: &mut Vec<String>,
    content_evidence: &ContentEvidence,
) -> Option<ScrapeResult> {
    let season_hint = preferred_season_hint(parsed, content_evidence);
    let episode_hint = preferred_episode_hint(parsed, content_evidence);
    let mut heuristic_scores: Vec<CandidateScore> = candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            score_candidate(
                parsed,
                media_type,
                candidate,
                idx,
                season_hint,
                episode_hint,
                content_evidence,
            )
        })
        .collect();
    heuristic_scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut embedding_scores = HashMap::new();
    if embedding_client.is_configured() {
        let query = format!(
            "{} {}",
            parsed.raw_title,
            parsed.year.map(|y| y.to_string()).unwrap_or_default()
        );
        if let Ok(ranked) = embedding_client.rerank(&query, candidates).await {
            for (idx, score) in ranked {
                embedding_scores.insert(idx, score as f32);
            }
        }
    }

    let best = heuristic_scores.into_iter().max_by(|a, b| {
        let a_total = a.score + embedding_scores.get(&a.index).copied().unwrap_or(0.0) * 0.12;
        let b_total = b.score + embedding_scores.get(&b.index).copied().unwrap_or(0.0) * 0.12;
        a_total
            .partial_cmp(&b_total)
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;

    let mut candidate = candidates.get(best.index).cloned()?;
    let embedding = embedding_scores.get(&best.index).copied();
    details.push(format!(
        "tmdb: selected candidate {} with heuristic {:.2}{}",
        best.index,
        best.score,
        embedding
            .map(|score| format!(", embedding {:.2}", score))
            .unwrap_or_default()
    ));
    for reason in &best.reasons {
        details.push(format!("tmdb: {reason}"));
        candidate.push_evidence(reason.clone());
    }
    candidate.confidence = candidate
        .confidence
        .max((0.62 + best.score * 0.18 + embedding.unwrap_or(0.0) * 0.08).clamp(0.62, 0.97));
    Some(candidate)
}

fn score_candidate(
    parsed: &ParsedInfo,
    media_type: MediaType,
    candidate: &ScrapeResult,
    index: usize,
    season_hint: Option<u32>,
    episode_hint: Option<u32>,
    content_evidence: &ContentEvidence,
) -> CandidateScore {
    let parsed_title = normalized_identity_tokens(&parsed.raw_title);
    let cand_title = normalized_identity_tokens(&candidate.title);
    let cand_original = candidate
        .title_original
        .as_deref()
        .map(normalized_identity_tokens)
        .unwrap_or_default();

    let mut score = 0.0;
    let mut reasons = vec![format!("candidate {index} title='{}'", candidate.title)];

    if !parsed_title.is_empty() && (cand_title == parsed_title || cand_original == parsed_title) {
        score += 1.2;
        reasons.push("exact normalized title match".into());
    } else if !parsed_title.is_empty()
        && (cand_title.contains(&parsed_title)
            || parsed_title.contains(&cand_title)
            || (!cand_original.is_empty() && cand_original.contains(&parsed_title)))
    {
        score += 0.7;
        reasons.push("partial normalized title match".into());
    }

    if let (Some(expected), Some(found)) = (parsed.year, candidate.year) {
        if expected == found {
            score += 0.45;
            reasons.push(format!("year exact match {expected}"));
        } else {
            let delta = expected.abs_diff(found);
            if delta == 1 {
                score += 0.12;
                reasons.push(format!("year near match {expected} vs {found}"));
            } else {
                score -= 0.25;
                reasons.push(format!("year mismatch {expected} vs {found}"));
            }
        }
    }

    if media_type == MediaType::TvShow {
        if season_hint.is_some() && candidate.tmdb_id.is_some() {
            score += 0.08;
            reasons.push("tv candidate has series id for episode enrichment".into());
        }
        if let (Some(expected), Some(found)) = (season_hint, candidate.season_number) {
            if expected == found {
                score += 0.18;
                reasons.push(format!("season hint match {expected}"));
            }
        }
        if let (Some(expected), Some(found)) = (episode_hint, candidate.episode_number) {
            if expected == found {
                score += 0.22;
                reasons.push(format!("episode hint match {expected}"));
            }
        }
    }

    for evidence_title in &content_evidence.title_candidates {
        let evidence_title = normalized_identity_tokens(evidence_title);
        if evidence_title.is_empty() {
            continue;
        }
        if evidence_title == cand_title || evidence_title == cand_original {
            score += 0.48;
            reasons.push(format!(
                "content evidence title matched '{}'",
                evidence_title
            ));
            break;
        } else if cand_title.contains(&evidence_title)
            || evidence_title.contains(&cand_title)
            || (!cand_original.is_empty() && cand_original.contains(&evidence_title))
        {
            score += 0.18;
            reasons.push(format!(
                "content evidence title partially matched '{}'",
                evidence_title
            ));
        }
    }

    if candidate.rating.unwrap_or(0.0) >= 7.0 {
        score += 0.05;
        reasons.push("high provider rating".into());
    }

    CandidateScore {
        index,
        score,
        reasons,
    }
}

fn preferred_season_hint(parsed: &ParsedInfo, content_evidence: &ContentEvidence) -> Option<u32> {
    parsed
        .season
        .or_else(|| content_evidence.season_hypotheses.first().copied())
}

fn preferred_episode_hint(parsed: &ParsedInfo, content_evidence: &ContentEvidence) -> Option<u32> {
    parsed
        .episode
        .or_else(|| content_evidence.episode_hypotheses.first().copied())
}

fn build_identity_candidates(
    parsed: &ParsedInfo,
    media_type: MediaType,
    candidates: &[ScrapeResult],
    season_hint: Option<u32>,
    episode_hint: Option<u32>,
    content_evidence: &ContentEvidence,
) -> Vec<IdentityCandidate> {
    let mut scored = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let score = score_candidate(
                parsed,
                media_type,
                candidate,
                index,
                season_hint,
                episode_hint,
                content_evidence,
            );
            IdentityCandidate {
                source: candidate.source,
                title: candidate.title.clone(),
                year: candidate.year,
                season: candidate.season_number,
                episode: candidate.episode_number,
                episode_title: candidate.episode_name.clone(),
                score: score.score,
                evidence: score.reasons,
            }
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored
}

fn resolve_identity(
    selected: &ScrapeResult,
    candidates: &[IdentityCandidate],
    media_type: MediaType,
    content_evidence: &ContentEvidence,
    runtime_secs: Option<u64>,
    risk_flags: &mut Vec<String>,
) -> IdentityResolution {
    let strong_sources = strong_evidence_count(content_evidence);
    let best = candidates
        .first()
        .cloned()
        .or_else(|| Some(identity_candidate_from_result(selected)));
    let ambiguous =
        candidates.len() > 1 && (candidates[0].score - candidates[1].score).abs() < 0.18;
    let corroboration = corroboration_score(&best, content_evidence);

    let confirmation_state = if best.is_none() || strong_sources == 0 {
        ConfirmationState::InsufficientEvidence
    } else if ambiguous {
        risk_flags.push("top candidates are too close to uniquely confirm identity".into());
        ConfirmationState::AmbiguousCandidates
    } else if strong_sources >= 2
        && corroboration >= 2
        && is_confirmable_match(media_type, &best, content_evidence)
    {
        ConfirmationState::Confirmed
    } else {
        ConfirmationState::HighConfidenceCandidate
    };

    let mut evidence_refs = Vec::new();
    evidence_refs.push(format!("strong_content_sources={strong_sources}"));
    evidence_refs.push(format!("corroboration_score={corroboration}"));
    if let Some(runtime_secs) = runtime_secs {
        evidence_refs.push(format!("runtime_secs={runtime_secs}"));
    }
    evidence_refs.extend(
        content_evidence
            .title_candidates
            .iter()
            .take(3)
            .map(|title| format!("title_candidate={title}")),
    );

    IdentityResolution {
        confirmation_state,
        best,
        candidates: candidates.to_vec(),
        evidence_refs,
        risk_flags: risk_flags.clone(),
    }
}

fn identity_candidate_from_result(result: &ScrapeResult) -> IdentityCandidate {
    IdentityCandidate {
        source: result.source,
        title: result.title.clone(),
        year: result.year,
        season: result.season_number,
        episode: result.episode_number,
        episode_title: result.episode_name.clone(),
        score: result.authority_score(),
        evidence: result.evidence.iter().take(6).cloned().collect(),
    }
}

fn strong_evidence_count(content_evidence: &ContentEvidence) -> usize {
    let mut count = 0;
    if content_evidence.container.title.is_some()
        || !content_evidence.container.chapters.is_empty()
        || !content_evidence.container.track_titles.is_empty()
    {
        count += 1;
    }
    if !content_evidence.subtitles.is_empty() {
        count += 1;
    }
    if !content_evidence.visual.is_empty()
        && content_evidence
            .visual
            .iter()
            .any(|visual| !visual.text_hits.is_empty())
    {
        count += 1;
    }
    if !content_evidence.audio.is_empty()
        && content_evidence
            .audio
            .iter()
            .any(|audio| !audio.transcript_hits.is_empty())
    {
        count += 1;
    }
    count
}

fn is_confirmable_match(
    media_type: MediaType,
    best: &Option<IdentityCandidate>,
    content_evidence: &ContentEvidence,
) -> bool {
    let Some(best) = best else {
        return false;
    };
    if best.score < 1.05 {
        return false;
    }
    match media_type {
        MediaType::TvShow => {
            let season_match = best
                .season
                .map(|season| content_evidence.season_hypotheses.contains(&season))
                .unwrap_or(false);
            let episode_match = best
                .episode
                .map(|episode| content_evidence.episode_hypotheses.contains(&episode))
                .unwrap_or(false);
            let title_match = title_evidence_matches_candidate(best, content_evidence);
            (season_match && episode_match)
                || (title_match && episode_match)
                || (title_match && season_match && best.episode_title.is_some())
        }
        MediaType::Movie => title_evidence_matches_candidate(best, content_evidence),
        _ => false,
    }
}

fn corroboration_score(
    best: &Option<IdentityCandidate>,
    content_evidence: &ContentEvidence,
) -> usize {
    let Some(best) = best else {
        return 0;
    };

    let mut score = 0;
    if title_evidence_matches_candidate(best, content_evidence) {
        score += 1;
    }
    if best
        .season
        .map(|season| content_evidence.season_hypotheses.contains(&season))
        .unwrap_or(false)
    {
        score += 1;
    }
    if best
        .episode
        .map(|episode| content_evidence.episode_hypotheses.contains(&episode))
        .unwrap_or(false)
    {
        score += 1;
    }
    score
}

fn title_evidence_matches_candidate(
    best: &IdentityCandidate,
    content_evidence: &ContentEvidence,
) -> bool {
    let best_title = normalized_identity_tokens(&best.title);
    let best_episode = best
        .episode_title
        .as_deref()
        .map(normalized_identity_tokens)
        .unwrap_or_default();
    content_evidence.title_candidates.iter().any(|title| {
        let normalized = normalized_identity_tokens(title);
        !normalized.is_empty()
            && (normalized == best_title
                || (!best_episode.is_empty() && normalized == best_episode)
                || best_title.contains(&normalized)
                || normalized.contains(&best_title))
    })
}

fn normalized_identity_tokens(raw: &str) -> String {
    normalize_title_query(&strip_title_noise(raw)).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{AiConfig, ApiConfig};
    use crate::models::media::{
        ConfirmationState, IdentityCandidate, ParseSource, ParsedInfo, SubtitleEvidence,
        SubtitleEvidenceSource,
    };

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
            confidence: 0.9,
            evidence: vec!["matched movie title + year pattern".into()],
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
            confidence: 0.9,
            evidence: vec!["matched movie title + year pattern".into()],
        };
        let tmdb = TmdbScraper::new(&ApiConfig::default());
        let mb = MusicBrainzScraper::new(&ApiConfig::default());
        let ol = OpenLibraryScraper::new();
        let ai_client = OpenAiCompat::from_config(&AiConfig::default());
        let embedding_client = EmbeddingClient::from_config(&AiConfig::default());
        let fallback_chain = ["guess".to_string()];
        let rt = crate::core::runtime::build().expect("runtime");
        let content_evidence = ContentEvidence::default();

        let request = ScrapeRequest {
            path: std::path::Path::new("/tmp/Arrival.2016.mkv"),
            parsed: &Some(parsed),
            quality: None,
            media_type: MediaType::Movie,
            content_evidence: &content_evidence,
        };
        let context = ScrapeContext {
            tmdb: &tmdb,
            mb: &mb,
            ol: &ol,
            ai_client: Some(&ai_client),
            embedding_client: &embedding_client,
            fallback_chain: &fallback_chain,
            chinese_priority: false,
        };

        let result = rt.block_on(scrape_with_fallback(request, &context));

        assert!(result.result.is_some());
        assert!(result.details.iter().any(|line| line.contains("guess")));
        let result = result.result.unwrap();
        assert_eq!(result.source, ScrapeSource::Guess);
        assert_eq!(result.title, "Arrival");
    }

    #[test]
    fn test_scrape_cache_key_distinguishes_tv_episodes() {
        let base = ParsedInfo {
            raw_title: "Love.Death.and.Robots".into(),
            year: Some(2025),
            season: Some(4),
            episode: Some(1),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.95,
            evidence: vec!["matched TV pattern SxxExx".into()],
        };
        let mut other = base.clone();
        other.episode = Some(2);

        assert_ne!(
            scrape_cache_key(&base, MediaType::TvShow),
            scrape_cache_key(&other, MediaType::TvShow)
        );
    }

    #[test]
    fn test_title_query_variants_normalize_noise() {
        let variants = title_query_variants("Inception.2010.1080p.BluRay.x264");
        assert!(
            variants
                .iter()
                .any(|v| v == "Inception 2010 1080p BluRay x264")
        );
        assert!(variants.iter().any(|v| v == "Inception 2010"));
        assert!(variants.iter().any(|v| v == "Inception"));
    }

    #[test]
    fn test_title_query_variants_strip_tv_identity_noise() {
        let variants = title_query_variants("Breaking.Bad.S01E02.1080p.WEB-DL");
        assert!(variants.iter().any(|v| v == "Breaking Bad S01E02"));
        assert!(variants.iter().any(|v| v == "Breaking Bad"));
    }

    #[test]
    fn test_score_candidate_prefers_exact_title_and_year() {
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
            confidence: 0.95,
            evidence: vec![],
        };
        let mut exact = ScrapeResult::empty(ScrapeSource::Tmdb, "Inception").with_confidence(0.9);
        exact.year = Some(2010);
        let mut wrong =
            ScrapeResult::empty(ScrapeSource::Tmdb, "Interstellar").with_confidence(0.9);
        wrong.year = Some(2014);
        let exact_score = score_candidate(
            &parsed,
            MediaType::Movie,
            &exact,
            0,
            None,
            None,
            &ContentEvidence::default(),
        );
        let wrong_score = score_candidate(
            &parsed,
            MediaType::Movie,
            &wrong,
            1,
            None,
            None,
            &ContentEvidence::default(),
        );
        assert!(exact_score.score > wrong_score.score);
    }

    #[test]
    fn test_guess_confidence_tracks_parse_confidence() {
        let low = ParsedInfo {
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
            confidence: 0.4,
            evidence: vec![],
        };
        let high = ParsedInfo {
            confidence: 0.95,
            ..low.clone()
        };
        let low_guess = guess_from_parsed(&low).unwrap();
        let high_guess = guess_from_parsed(&high).unwrap();
        assert!(high_guess.confidence > low_guess.confidence);
    }

    #[test]
    fn test_contextual_title_queries_include_parent_show_name() {
        let parsed = ParsedInfo {
            raw_title: "01".into(),
            year: None,
            season: Some(1),
            episode: Some(1),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Context,
            confidence: 0.7,
            evidence: vec![],
        };
        let queries = contextual_title_queries(
            std::path::Path::new("/media/Breaking Bad/Season 01/01.mkv"),
            &parsed,
            &ContentEvidence::default(),
        );
        assert!(queries.iter().any(|q| q == "Breaking Bad"));
    }

    #[test]
    fn test_resolve_identity_confirms_tv_with_multi_evidence_alignment() {
        let selected =
            ScrapeResult::empty(ScrapeSource::Tmdb, "Breaking Bad").with_confidence(0.95);
        let candidates = vec![IdentityCandidate {
            source: ScrapeSource::Tmdb,
            title: "Breaking Bad".into(),
            year: Some(2008),
            season: Some(1),
            episode: Some(3),
            episode_title: Some("...And the Bag's in the River".into()),
            score: 1.72,
            evidence: vec![],
        }];
        let content = ContentEvidence {
            container: crate::models::media::ContainerEvidence {
                title: Some("Breaking Bad".into()),
                ..Default::default()
            },
            subtitles: vec![SubtitleEvidence {
                source: SubtitleEvidenceSource::EmbeddedTrack,
                locator: "embedded:stream:2".into(),
                language: Some("eng".into()),
                track_title: None,
                sample_lines: vec!["Previously on Breaking Bad".into()],
                title_candidates: vec!["Breaking Bad".into()],
                season: Some(1),
                episode: Some(3),
            }],
            title_candidates: vec!["Breaking Bad".into()],
            season_hypotheses: vec![1],
            episode_hypotheses: vec![3],
            ..Default::default()
        };

        let mut risks = Vec::new();
        let resolution = resolve_identity(
            &selected,
            &candidates,
            MediaType::TvShow,
            &content,
            None,
            &mut risks,
        );

        assert_eq!(resolution.confirmation_state, ConfirmationState::Confirmed);
    }

    #[test]
    fn test_resolve_identity_marks_ambiguous_close_candidates() {
        let selected = ScrapeResult::empty(ScrapeSource::Tmdb, "Dark").with_confidence(0.95);
        let candidates = vec![
            IdentityCandidate {
                source: ScrapeSource::Tmdb,
                title: "Dark".into(),
                year: Some(2017),
                season: None,
                episode: None,
                episode_title: None,
                score: 1.31,
                evidence: vec![],
            },
            IdentityCandidate {
                source: ScrapeSource::Tmdb,
                title: "Dark".into(),
                year: Some(2019),
                season: None,
                episode: None,
                episode_title: None,
                score: 1.21,
                evidence: vec![],
            },
        ];
        let content = ContentEvidence {
            title_candidates: vec!["Dark".into()],
            subtitles: vec![SubtitleEvidence {
                source: SubtitleEvidenceSource::ExternalText,
                locator: "/tmp/dark.srt".into(),
                language: None,
                track_title: None,
                sample_lines: vec!["Previously on Dark".into()],
                title_candidates: vec!["Dark".into()],
                season: None,
                episode: None,
            }],
            ..Default::default()
        };

        let mut risks = Vec::new();
        let resolution = resolve_identity(
            &selected,
            &candidates,
            MediaType::Movie,
            &content,
            None,
            &mut risks,
        );

        assert_eq!(
            resolution.confirmation_state,
            ConfirmationState::AmbiguousCandidates
        );
        assert!(!resolution.risk_flags.is_empty());
    }

    #[test]
    fn test_resolve_identity_requires_strong_content_sources() {
        let selected = ScrapeResult::empty(ScrapeSource::Tmdb, "Arrival").with_confidence(0.95);
        let candidates = vec![IdentityCandidate {
            source: ScrapeSource::Tmdb,
            title: "Arrival".into(),
            year: Some(2016),
            season: None,
            episode: None,
            episode_title: None,
            score: 1.6,
            evidence: vec![],
        }];

        let mut risks = Vec::new();
        let resolution = resolve_identity(
            &selected,
            &candidates,
            MediaType::Movie,
            &ContentEvidence::default(),
            None,
            &mut risks,
        );

        assert_eq!(
            resolution.confirmation_state,
            ConfirmationState::InsufficientEvidence
        );
    }
}
