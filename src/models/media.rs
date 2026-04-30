use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 媒体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    Movie,
    TvShow,
    Music,
    Novel,
    Strm,
    Unknown,
}

/// 媒体文件条目 — 扫描后的完整信息载体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: u64,
    pub path: PathBuf,
    pub file_size: u64,
    pub media_type: MediaType,
    pub extension: String,

    pub parsed: Option<ParsedInfo>,
    pub quality: Option<QualityInfo>,
    pub scraped: Option<ScrapeResult>,
    #[serde(default)]
    pub content_evidence: Option<ContentEvidence>,
    #[serde(default)]
    pub identity_resolution: Option<IdentityResolution>,
    pub hash: Option<HashInfo>,
    pub rename_plan: Option<RenamePlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetadataOrigin {
    Parsed,
    Scraped,
}

#[derive(Debug, Clone)]
pub struct MetadataDecision<T> {
    pub value: T,
    pub origin: MetadataOrigin,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanIndex {
    pub root: PathBuf,
    pub items: Vec<MediaItem>,
}

/// 文件名解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedInfo {
    pub raw_title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub resolution: Option<String>,
    pub codec: Option<String>,
    pub source: Option<String>,
    pub release_group: Option<String>,
    pub media_suffix: Option<String>,
    pub parse_source: ParseSource,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseSource {
    Regex,
    AiAssist,
    Context,
}

/// 媒体质量信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityInfo {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub resolution_label: String,
    pub video_codec: Option<String>,
    pub video_bitrate: Option<u64>,
    pub audio_codec: Option<String>,
    pub audio_bitrate: Option<u64>,
    pub duration_secs: Option<u64>,
    pub quality_score: f64,
    pub probe_source: ProbeSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeSource {
    Native,
    Ffprobe,
}

/// 哈希信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashInfo {
    pub size_hash: u64,
    pub prefix_hash: Option<u64>,
    pub full_hash: Option<u64>,
}

/// 刮削结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResult {
    pub source: ScrapeSource,
    pub title: String,
    pub title_original: Option<String>,
    pub year: Option<u16>,
    pub overview: Option<String>,
    pub rating: Option<f64>,

    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub episode_name: Option<String>,
    pub poster_url: Option<String>,
    pub fanart_url: Option<String>,

    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,

    pub author: Option<String>,
    pub cover_url: Option<String>,

    pub tmdb_id: Option<u64>,
    pub musicbrainz_id: Option<String>,
    pub openlibrary_id: Option<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrapeSource {
    LocalNfo,
    Tmdb,
    MusicBrainz,
    OpenLibrary,
    AiAssist,
    Guess,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentEvidence {
    #[serde(default)]
    pub container: ContainerEvidence,
    #[serde(default)]
    pub subtitles: Vec<SubtitleEvidence>,
    #[serde(default)]
    pub visual: Vec<VisualEvidence>,
    #[serde(default)]
    pub audio: Vec<AudioEvidence>,
    #[serde(default)]
    pub title_candidates: Vec<String>,
    #[serde(default)]
    pub season_hypotheses: Vec<u32>,
    #[serde(default)]
    pub episode_hypotheses: Vec<u32>,
    pub runtime_secs: Option<u64>,
    #[serde(default)]
    pub risk_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContainerEvidence {
    pub format_name: Option<String>,
    pub title: Option<String>,
    pub comment: Option<String>,
    #[serde(default)]
    pub chapters: Vec<String>,
    #[serde(default)]
    pub stream_languages: Vec<String>,
    #[serde(default)]
    pub track_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleEvidence {
    pub source: SubtitleEvidenceSource,
    pub locator: String,
    pub language: Option<String>,
    pub track_title: Option<String>,
    #[serde(default)]
    pub sample_lines: Vec<String>,
    #[serde(default)]
    pub title_candidates: Vec<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubtitleEvidenceSource {
    ExternalText,
    EmbeddedTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VisualEvidence {
    pub source: String,
    #[serde(default)]
    pub text_hits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioEvidence {
    pub source: String,
    #[serde(default)]
    pub transcript_hits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCandidate {
    pub source: ScrapeSource,
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
    pub score: f32,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationState {
    Confirmed,
    HighConfidenceCandidate,
    AmbiguousCandidates,
    InsufficientEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityResolution {
    pub confirmation_state: ConfirmationState,
    pub best: Option<IdentityCandidate>,
    #[serde(default)]
    pub candidates: Vec<IdentityCandidate>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub risk_flags: Vec<String>,
}

/// 重命名计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamePlan {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub subtitle_plans: Vec<SubtitleRenamePlan>,
    pub directory_plans: Vec<DirectoryRenamePlan>,
    #[serde(default)]
    pub decision: RenameDecisionProfile,
    pub rationale: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenameDecisionProfile {
    pub template: String,
    pub template_uses_ext: bool,
    pub preserve_media_suffix: bool,
    pub title_origin: Option<MetadataOrigin>,
    pub title_confidence: Option<f32>,
    pub year_origin: Option<MetadataOrigin>,
    pub year_confidence: Option<f32>,
    pub season_origin: Option<MetadataOrigin>,
    pub season_confidence: Option<f32>,
    pub episode_origin: Option<MetadataOrigin>,
    pub episode_confidence: Option<f32>,
    pub subtitle_count: usize,
    pub directory_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleRenamePlan {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryRenamePlan {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
}

impl QualityInfo {
    pub fn new(source: ProbeSource) -> Self {
        Self {
            width: None,
            height: None,
            resolution_label: "unknown".into(),
            video_codec: None,
            video_bitrate: None,
            audio_codec: None,
            audio_bitrate: None,
            duration_secs: None,
            quality_score: 0.0,
            probe_source: source,
        }
    }
}

impl ParsedInfo {
    pub fn push_evidence(&mut self, detail: impl Into<String>) {
        self.evidence.push(detail.into());
    }

    pub fn bump_confidence(&mut self, confidence: f32) {
        if confidence > self.confidence {
            self.confidence = confidence;
        }
    }
}

impl ScrapeResult {
    pub fn empty(source: ScrapeSource, title: impl Into<String>) -> Self {
        Self {
            source,
            title: title.into(),
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
            confidence: 0.0,
            evidence: Vec::new(),
        }
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn with_evidence<I, S>(mut self, evidence: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.evidence = evidence.into_iter().map(Into::into).collect();
        self
    }

    pub fn push_evidence(&mut self, detail: impl Into<String>) {
        self.evidence.push(detail.into());
    }
}

impl ParsedInfo {
    pub fn authority_score(&self) -> f32 {
        self.confidence.clamp(0.0, 1.0)
    }
}

impl ScrapeResult {
    pub fn authority_score(&self) -> f32 {
        let source_bonus = match self.source {
            ScrapeSource::LocalNfo => 0.08,
            ScrapeSource::Tmdb | ScrapeSource::MusicBrainz | ScrapeSource::OpenLibrary => 0.06,
            ScrapeSource::AiAssist => -0.05,
            ScrapeSource::Guess => -0.12,
        };
        (self.confidence + source_bonus).clamp(0.0, 1.0)
    }
}

impl MediaItem {
    pub fn preferred_title(&self) -> Option<MetadataDecision<String>> {
        choose_text_value(
            self.parsed.as_ref().map(|parsed| {
                (
                    parsed.raw_title.as_str(),
                    parsed.authority_score(),
                    "parsed filename/title inference".to_string(),
                )
            }),
            self.scraped.as_ref().map(|scraped| {
                (
                    scraped.title.as_str(),
                    self.scraped_metadata_confidence(),
                    self.scraped_metadata_reason(scraped, "metadata"),
                )
            }),
        )
    }

    pub fn preferred_year(&self) -> Option<MetadataDecision<u16>> {
        choose_copy_value(
            self.parsed.as_ref().and_then(|parsed| {
                parsed
                    .year
                    .map(|year| (year, parsed.authority_score(), "parsed year".to_string()))
            }),
            self.scraped.as_ref().and_then(|scraped| {
                scraped.year.map(|year| {
                    (
                        year,
                        self.scraped_metadata_confidence(),
                        self.scraped_metadata_reason(scraped, "year"),
                    )
                })
            }),
        )
    }

    pub fn preferred_season(&self) -> Option<MetadataDecision<u32>> {
        choose_copy_value(
            self.parsed.as_ref().and_then(|parsed| {
                parsed.season.map(|season| {
                    (
                        season,
                        parsed.authority_score(),
                        "parsed season".to_string(),
                    )
                })
            }),
            self.scraped.as_ref().and_then(|scraped| {
                scraped.season_number.map(|season| {
                    (
                        season,
                        self.scraped_metadata_confidence(),
                        self.scraped_metadata_reason(scraped, "season"),
                    )
                })
            }),
        )
    }

    pub fn preferred_episode(&self) -> Option<MetadataDecision<u32>> {
        choose_copy_value(
            self.parsed.as_ref().and_then(|parsed| {
                parsed.episode.map(|episode| {
                    (
                        episode,
                        parsed.authority_score(),
                        "parsed episode".to_string(),
                    )
                })
            }),
            self.scraped.as_ref().and_then(|scraped| {
                scraped.episode_number.map(|episode| {
                    (
                        episode,
                        self.scraped_metadata_confidence(),
                        self.scraped_metadata_reason(scraped, "episode"),
                    )
                })
            }),
        )
    }

    pub fn preferred_metadata_confidence(&self) -> f32 {
        self.preferred_title()
            .map(|decision| decision.confidence)
            .or_else(|| {
                self.scraped
                    .as_ref()
                    .map(|_| self.scraped_metadata_confidence())
            })
            .or_else(|| self.parsed.as_ref().map(|parsed| parsed.authority_score()))
            .unwrap_or(0.0)
    }

    pub fn scraped_metadata_confidence(&self) -> f32 {
        let base = self
            .scraped
            .as_ref()
            .map(|scraped| scraped.authority_score())
            .unwrap_or(0.0);
        (base + self.identity_confirmation_adjustment()).clamp(0.0, 1.0)
    }

    pub fn identity_trust_score(&self) -> f32 {
        match self.identity_confirmation_state() {
            Some(ConfirmationState::Confirmed) => 1.0,
            Some(ConfirmationState::HighConfidenceCandidate) => 0.82,
            Some(ConfirmationState::AmbiguousCandidates) => 0.42,
            Some(ConfirmationState::InsufficientEvidence) => 0.18,
            None => 0.0,
        }
    }

    pub fn identity_confirmation_state(&self) -> Option<ConfirmationState> {
        self.identity_resolution
            .as_ref()
            .map(|identity| identity.confirmation_state)
    }

    pub fn identity_confirmation_label(&self) -> Option<&'static str> {
        match self.identity_confirmation_state() {
            Some(ConfirmationState::Confirmed) => Some("confirmed"),
            Some(ConfirmationState::HighConfidenceCandidate) => Some("high-confidence"),
            Some(ConfirmationState::AmbiguousCandidates) => Some("ambiguous"),
            Some(ConfirmationState::InsufficientEvidence) => Some("insufficient"),
            None => None,
        }
    }

    pub fn canonical_nfo_authority_threshold(&self) -> Option<f32> {
        let scraped = self.scraped.as_ref()?;
        match self.identity_confirmation_state() {
            Some(ConfirmationState::Confirmed) => Some(0.78),
            Some(ConfirmationState::HighConfidenceCandidate)
                if !matches!(scraped.source, ScrapeSource::Guess | ScrapeSource::AiAssist) =>
            {
                Some(0.92)
            }
            _ => None,
        }
    }

    pub fn canonical_asset_authority_threshold(&self) -> Option<f32> {
        let scraped = self.scraped.as_ref()?;
        match self.identity_confirmation_state() {
            Some(ConfirmationState::Confirmed) => Some(0.82),
            Some(ConfirmationState::HighConfidenceCandidate)
                if !matches!(scraped.source, ScrapeSource::Guess | ScrapeSource::AiAssist) =>
            {
                Some(0.90)
            }
            _ => None,
        }
    }

    fn identity_confirmation_adjustment(&self) -> f32 {
        match self.identity_confirmation_state() {
            Some(ConfirmationState::Confirmed) => 0.18,
            Some(ConfirmationState::HighConfidenceCandidate) => 0.10,
            Some(ConfirmationState::AmbiguousCandidates) => -0.08,
            Some(ConfirmationState::InsufficientEvidence) => -0.14,
            None => 0.0,
        }
    }

    fn scraped_metadata_reason(&self, scraped: &ScrapeResult, field: &str) -> String {
        match self.identity_confirmation_label() {
            Some(label) => format!(
                "scraped {:?} {field} with {label} identity resolution",
                scraped.source
            ),
            None => format!("scraped {:?} {field}", scraped.source),
        }
    }
}

fn choose_text_value(
    parsed: Option<(&str, f32, String)>,
    scraped: Option<(&str, f32, String)>,
) -> Option<MetadataDecision<String>> {
    choose_value(
        parsed.filter(|(value, _, _)| !value.trim().is_empty()).map(
            |(value, confidence, reason)| MetadataDecision {
                value: value.to_string(),
                origin: MetadataOrigin::Parsed,
                confidence,
                reason,
            },
        ),
        scraped
            .filter(|(value, _, _)| !value.trim().is_empty())
            .map(|(value, confidence, reason)| MetadataDecision {
                value: value.to_string(),
                origin: MetadataOrigin::Scraped,
                confidence,
                reason,
            }),
    )
}

fn choose_copy_value<T: Copy>(
    parsed: Option<(T, f32, String)>,
    scraped: Option<(T, f32, String)>,
) -> Option<MetadataDecision<T>> {
    choose_value(
        parsed.map(|(value, confidence, reason)| MetadataDecision {
            value,
            origin: MetadataOrigin::Parsed,
            confidence,
            reason,
        }),
        scraped.map(|(value, confidence, reason)| MetadataDecision {
            value,
            origin: MetadataOrigin::Scraped,
            confidence,
            reason,
        }),
    )
}

fn choose_value<T>(
    parsed: Option<MetadataDecision<T>>,
    scraped: Option<MetadataDecision<T>>,
) -> Option<MetadataDecision<T>> {
    match (parsed, scraped) {
        (Some(parsed), Some(scraped)) => {
            let scraped_trusted = scraped.confidence >= 0.72;
            let scraped_beats_parsed = scraped.confidence + 0.03 >= parsed.confidence;
            if scraped_trusted && scraped_beats_parsed {
                Some(scraped)
            } else {
                Some(parsed)
            }
        }
        (Some(parsed), None) => Some(parsed),
        (None, Some(scraped)) => Some(scraped),
        (None, None) => None,
    }
}
