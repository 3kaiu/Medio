use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::hasher::FileHasher;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::db::cache::Cache;
use crate::media::ffprobe::FfprobeProbe;
use crate::media::native_probe::NativeProbe;
use crate::media::probe::MediaProbe;
use crate::models::media::{MediaItem, ScanIndex};
use crate::scraper;
use rayon::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSource {
    LiveScan,
    CachedIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeBackend {
    Auto,
    Native,
    Ffprobe,
}

impl ProbeBackend {
    pub fn from_cli(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "native" => Self::Native,
            "ffprobe" => Self::Ffprobe,
            _ => Self::Auto,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Native => "native",
            Self::Ffprobe => "ffprobe",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StageReport {
    pub stage: String,
    pub item_count: usize,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineState {
    pub root: PathBuf,
    pub item_source: String,
    pub items: Vec<MediaItem>,
    pub stages: Vec<StageReport>,
}

impl PipelineState {
    fn new(root: PathBuf, item_source: ItemSource, items: Vec<MediaItem>) -> Self {
        Self {
            root,
            item_source: match item_source {
                ItemSource::LiveScan => "live_scan".into(),
                ItemSource::CachedIndex => "cached_index".into(),
            },
            items,
            stages: Vec::new(),
        }
    }

    fn push_stage<S: Into<String>>(&mut self, stage: S, details: Vec<String>) {
        self.stages.push(StageReport {
            stage: stage.into(),
            item_count: self.items.len(),
            details,
        });
    }
}

pub struct Pipeline<'a> {
    config: &'a AppConfig,
}

impl<'a> Pipeline<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn scan_root(&self, root: &Path) -> Result<PipelineState, String> {
        if !root.exists() {
            return Err(format!("Error: path does not exist: {}", root.display()));
        }
        if !root.is_dir() {
            return Err(format!(
                "Error: path is not a directory: {}",
                root.display()
            ));
        }

        let scanner = Scanner::new(self.config.scan.clone());
        let items = scanner.scan(root);
        let mut state = PipelineState::new(root.to_path_buf(), ItemSource::LiveScan, items);
        self.persist_scan_index(root, &state.items);

        state.push_stage(
            "discover",
            vec![
                "source: live_scan".into(),
                format!("root: {}", root.display()),
                format!("items: {}", state.items.len()),
            ],
        );
        Ok(state)
    }

    pub fn load_or_scan(&self, root: &Path) -> Result<PipelineState, String> {
        if !root.exists() {
            return Err(format!("Error: path does not exist: {}", root.display()));
        }

        if let Some(items) = self.load_scan_index(root) {
            let mut state = PipelineState::new(root.to_path_buf(), ItemSource::CachedIndex, items);
            state.push_stage(
                "discover",
                vec![
                    "source: cached_index".into(),
                    format!("root: {}", root.display()),
                    format!("items: {}", state.items.len()),
                ],
            );
            return Ok(state);
        }

        if root.is_dir() {
            self.scan_root(root)
        } else {
            let scan_root = root.parent().unwrap_or(Path::new("."));
            let scanner = Scanner::new(self.config.scan.clone());
            let items: Vec<MediaItem> = scanner
                .scan(scan_root)
                .into_iter()
                .filter(|item| item.path == root)
                .collect();
            let mut state = PipelineState::new(root.to_path_buf(), ItemSource::LiveScan, items);
            state.push_stage(
                "discover",
                vec![
                    "source: live_scan".into(),
                    format!("target: {}", root.display()),
                    format!("items: {}", state.items.len()),
                ],
            );
            Ok(state)
        }
    }

    pub fn identify(&self, state: &mut PipelineState) {
        let keyword_filter = KeywordFilter::new(self.config.scan.keyword_filter.clone());
        let identifier = Identifier::new(keyword_filter);
        let before_tv = state
            .items
            .iter()
            .filter(|item| matches!(item.media_type, crate::models::media::MediaType::TvShow))
            .count();
        identifier.parse_batch(&mut state.items);
        let parsed = state
            .items
            .iter()
            .filter(|item| item.parsed.is_some())
            .count();
        let after_tv = state
            .items
            .iter()
            .filter(|item| matches!(item.media_type, crate::models::media::MediaType::TvShow))
            .count();

        state.push_stage(
            "identify",
            vec![
                format!("parsed_items: {parsed}"),
                format!("tv_promotions: {}", after_tv.saturating_sub(before_tv)),
            ],
        );
    }

    pub fn infer_context(&self, state: &mut PipelineState) {
        let before_context = state
            .items
            .iter()
            .filter(|item| {
                item.parsed
                    .as_ref()
                    .map(|parsed| {
                        matches!(
                            parsed.parse_source,
                            crate::models::media::ParseSource::Context
                        )
                    })
                    .unwrap_or(false)
            })
            .count();

        for item in &mut state.items {
            ContextInfer::enrich_item(item);
        }

        let after_context = state
            .items
            .iter()
            .filter(|item| {
                item.parsed
                    .as_ref()
                    .map(|parsed| {
                        matches!(
                            parsed.parse_source,
                            crate::models::media::ParseSource::Context
                        )
                    })
                    .unwrap_or(false)
            })
            .count();

        state.push_stage(
            "context",
            vec![
                format!(
                    "context_enriched: {}",
                    after_context.saturating_sub(before_context)
                ),
                format!("items: {}", state.items.len()),
            ],
        );
    }

    pub async fn scrape(&self, state: &mut PipelineState) {
        let traces = scraper::populate_scrape_results(&mut state.items, self.config).await;
        let scraped = state
            .items
            .iter()
            .filter(|item| item.scraped.is_some())
            .count();
        let mut details = vec![
            format!("scraped_items: {scraped}"),
            format!(
                "fallback_chain: {}",
                self.config.scrape.fallback_chain.join(" -> ")
            ),
        ];
        if state.items.len() == 1
            && let Some(trace) = traces.first()
        {
            details.extend(trace.details.clone());
            if let Some(content) = state.items[0].content_evidence.as_ref() {
                details.push(format!(
                    "content_probe titles={} subtitles={}",
                    content.title_candidates.len(),
                    content.subtitles.len()
                ));
            }
            if let Some(resolution) = state.items[0].identity_resolution.as_ref() {
                details.push(format!(
                    "identity_resolution: {:?}",
                    resolution.confirmation_state
                ));
            }
        }
        state.push_stage("scrape", details);
    }

    pub fn hash(&self, state: &mut PipelineState) {
        let cache = Cache::open(&self.config.cache_path()).ok();
        if let Some(ref cache) = cache {
            let _ = cache.cleanup(self.config.cache.ttl_days);
        }
        FileHasher::compute_all_with_cache(&mut state.items, cache.as_ref());

        let hashed = state
            .items
            .iter()
            .filter(|item| item.hash.is_some())
            .count();
        let full = state
            .items
            .iter()
            .filter(|item| item.hash.as_ref().and_then(|hash| hash.full_hash).is_some())
            .count();

        state.push_stage(
            "hash",
            vec![
                format!("hashed_items: {hashed}"),
                format!("full_hashes: {full}"),
            ],
        );
    }

    pub fn probe(&self, state: &mut PipelineState, backend: ProbeBackend) {
        let use_ffprobe = match backend {
            ProbeBackend::Ffprobe => FfprobeProbe::is_available(),
            ProbeBackend::Native => false,
            ProbeBackend::Auto => !self.config.general.dry_run && FfprobeProbe::is_available(),
        };

        if use_ffprobe {
            let probe = FfprobeProbe::new(self.config.quality.clone());
            state.items.par_iter_mut().for_each(|item| {
                if let Ok(quality) = probe.probe(&item.path) {
                    item.quality = Some(quality);
                }
            });
        } else {
            let probe = NativeProbe::new(self.config.quality.clone());
            state.items.par_iter_mut().for_each(|item| {
                if let Ok(quality) = probe.probe(&item.path) {
                    item.quality = Some(quality);
                }
            });
        }

        let probed = state
            .items
            .iter()
            .filter(|item| item.quality.is_some())
            .count();
        state.push_stage(
            "probe",
            vec![
                format!(
                    "backend: {}",
                    if use_ffprobe {
                        "ffprobe"
                    } else {
                        backend.label()
                    }
                ),
                format!("probed_items: {probed}"),
            ],
        );
    }

    fn load_scan_index(&self, root: &Path) -> Option<Vec<MediaItem>> {
        let root_key = root.to_str()?;
        let cache = Cache::open(&self.config.cache_path()).ok()?;
        let (updated_at, index) = cache.get_scan_index_entry(root_key)?;
        let root_mtime = root
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());

        match root_mtime {
            Some(root_mtime) if root_mtime > updated_at => None,
            _ => Some(index.items),
        }
    }

    fn persist_scan_index(&self, root: &Path, items: &[MediaItem]) {
        let cache_path = self.config.cache_path();
        let Some(root_key) = root.to_str() else {
            return;
        };
        let Ok(cache) = Cache::open(&cache_path) else {
            return;
        };
        let index = ScanIndex {
            root: root.to_path_buf(),
            items: items.to_vec(),
        };
        if cache.set_scan_index(root_key, &index).is_ok() {
            let _ = cache.flush();
        }
    }
}
