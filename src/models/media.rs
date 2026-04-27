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
    pub hash: Option<HashInfo>,
    pub rename_plan: Option<RenamePlan>,
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

/// 重命名计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamePlan {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub subtitle_plans: Vec<SubtitleRenamePlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleRenamePlan {
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
