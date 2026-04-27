use crate::core::types::{AiProvider, DupAction, KeepStrategy, LinkMode, OrganizeMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub scrape: ScrapeConfig,
    #[serde(default)]
    pub dedup: DedupConfig,
    #[serde(default)]
    pub rename: RenameConfig,
    #[serde(default)]
    pub organize: OrganizeConfig,
    #[serde(default)]
    pub quality: QualityConfig,
    #[serde(default)]
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub dry_run: bool,

    #[serde(default = "default_true")]
    pub confirm: bool,
    #[serde(default = "default_true")]
    pub operation_log: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default = "default_exclude_dirs")]
    pub exclude_dirs: Vec<String>,
    #[serde(default = "default_min_file_size")]
    pub min_file_size: u64,
    #[serde(default)]
    pub keyword_filter: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_ai_provider")]
    pub provider: AiProvider,
    #[serde(default = "default_deepseek")]
    pub deepseek: DeepSeekConfig,
    #[serde(default = "default_cloudflare")]
    pub cloudflare: CloudflareConfig,
    #[serde(default)]
    pub custom: CustomAiConfig,
    #[serde(default = "default_ai_provider")]
    pub embedding_provider: AiProvider,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekConfig {
    #[serde(default = "default_deepseek_url")]
    pub url: String,
    #[serde(default)]
    pub key: String,
    #[serde(default = "default_deepseek_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareConfig {
    #[serde(default = "default_cf_url")]
    pub url: String,
    #[serde(default)]
    pub api_token: String,
    #[serde(default = "default_cf_model")]
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomAiConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default)]
    pub tmdb_key: String,
    #[serde(default)]
    pub musicbrainz_user_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeConfig {
    #[serde(default = "default_fallback_chain")]
    pub fallback_chain: Vec<String>,
    #[serde(default = "default_true")]
    pub chinese_title_priority: bool,
    #[serde(default)]
    pub with_nfo: bool,
    #[serde(default)]
    pub with_images: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupConfig {
    #[serde(default = "default_hash_algo")]
    pub hash_algorithm: String,
    #[serde(default = "default_keep_strategy")]
    pub keep_strategy: KeepStrategy,
    #[serde(default = "default_dup_action")]
    pub duplicate_action: DupAction,
    #[serde(default)]
    pub move_target: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameConfig {
    #[serde(default = "default_movie_template")]
    pub movie_template: String,
    #[serde(default = "default_tv_template")]
    pub tv_template: String,
    #[serde(default = "default_music_template")]
    pub music_template: String,
    #[serde(default = "default_novel_template")]
    pub novel_template: String,
    #[serde(default = "default_true")]
    pub preserve_media_suffix: bool,
    #[serde(default)]
    pub season_offset: i32,
    #[serde(default = "default_true")]
    pub rename_subtitles: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeConfig {
    #[serde(default = "default_organize_mode")]
    pub mode: OrganizeMode,
    #[serde(default)]
    pub root: PathBuf,
    #[serde(default = "default_link_mode")]
    pub link_mode: LinkMode,
    #[serde(default)]
    pub with_nfo: bool,
    #[serde(default)]
    pub with_images: bool,
    #[serde(default = "default_true")]
    pub cleanup_empty_dirs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    #[serde(default = "default_res_weight")]
    pub resolution_weight: f64,
    #[serde(default = "default_codec_weight")]
    pub codec_weight: f64,
    #[serde(default = "default_bitrate_weight")]
    pub bitrate_weight: f64,
    #[serde(default = "default_audio_weight")]
    pub audio_weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub path: PathBuf,
    #[serde(default = "default_ttl")]
    pub ttl_days: u64,
}

// --- Default functions ---

fn default_true() -> bool { true }
fn default_log_level() -> String { "info".into() }
fn default_max_depth() -> usize { 10 }
fn default_min_file_size() -> u64 { 1_048_576 } // 1MB
fn default_exclude_dirs() -> Vec<String> {
    vec![
        ".git".into(), ".DS_Store".into(), "Thumbs.db".into(), "__MACOSX".into(),
        "Library".into(), ".Trash".into(), ".cache".into(),
        ".npm".into(), ".cargo".into(), ".rustup".into(),
        "node_modules".into(),
    ]
}
fn default_ai_provider() -> AiProvider { AiProvider::DeepSeek }
fn default_deepseek() -> DeepSeekConfig {
    DeepSeekConfig {
        url: default_deepseek_url(),
        key: String::new(),
        model: default_deepseek_model(),
    }
}
fn default_deepseek_url() -> String { "https://api.deepseek.com/v1".into() }
fn default_deepseek_model() -> String { "deepseek-chat".into() }
fn default_cloudflare() -> CloudflareConfig {
    CloudflareConfig {
        url: default_cf_url(),
        api_token: String::new(),
        model: default_cf_model(),
    }
}
fn default_cf_url() -> String {
    "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai".into()
}
fn default_cf_model() -> String { "@cf/meta/llama-3.1-8b-instruct".into() }
fn default_concurrency() -> usize { 3 }
fn default_fallback_chain() -> Vec<String> {
    vec!["local".into(), "tmdb".into(), "musicbrainz".into(), "ai".into(), "guess".into()]
}
fn default_hash_algo() -> String { "xxhash".into() }
fn default_keep_strategy() -> KeepStrategy { KeepStrategy::HighestQuality }
fn default_dup_action() -> DupAction { DupAction::Trash }
fn default_movie_template() -> String {
    "{{title}}{{year}} - {{media_suffix}}".into()
}
fn default_tv_template() -> String {
    "{{title}} - S{{season}}E{{episode}} - {{episode_name}}".into()
}
fn default_music_template() -> String {
    "{{artist}} - {{album}} - {{title}}".into()
}
fn default_novel_template() -> String {
    "{{author}} - {{title}}".into()
}
fn default_organize_mode() -> OrganizeMode { OrganizeMode::Archive }
fn default_link_mode() -> LinkMode { LinkMode::None }
fn default_res_weight() -> f64 { 0.4 }
fn default_codec_weight() -> f64 { 0.3 }
fn default_bitrate_weight() -> f64 { 0.2 }
fn default_audio_weight() -> f64 { 0.1 }
fn default_ttl() -> u64 { 90 }

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            dry_run: true,
            confirm: true,
            operation_log: true,
            log_level: default_log_level(),
        }
    }
}
impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
            follow_symlinks: false,
            exclude_dirs: default_exclude_dirs(),
            min_file_size: default_min_file_size(),
            keyword_filter: Vec::new(),
        }
    }
}
impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: default_ai_provider(),
            deepseek: default_deepseek(),
            cloudflare: default_cloudflare(),
            custom: CustomAiConfig { url: String::new(), key: String::new(), model: String::new() },
            embedding_provider: default_ai_provider(),
            embedding_model: String::new(),
            concurrency: default_concurrency(),
        }
    }
}
impl Default for ApiConfig {
    fn default() -> Self { Self { tmdb_key: String::new(), musicbrainz_user_agent: String::new() } }
}
impl Default for ScrapeConfig {
    fn default() -> Self {
        Self {
            fallback_chain: default_fallback_chain(),
            chinese_title_priority: true,
            with_nfo: false,
            with_images: false,
        }
    }
}
impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            hash_algorithm: default_hash_algo(),
            keep_strategy: default_keep_strategy(),
            duplicate_action: default_dup_action(),
            move_target: PathBuf::new(),
        }
    }
}
impl Default for RenameConfig {
    fn default() -> Self {
        Self {
            movie_template: default_movie_template(),
            tv_template: default_tv_template(),
            music_template: default_music_template(),
            novel_template: default_novel_template(),
            preserve_media_suffix: true,
            season_offset: 0,
            rename_subtitles: true,
        }
    }
}
impl Default for OrganizeConfig {
    fn default() -> Self {
        Self {
            mode: default_organize_mode(),
            root: PathBuf::new(),
            link_mode: default_link_mode(),
            with_nfo: false,
            with_images: false,
            cleanup_empty_dirs: true,
        }
    }
}
impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            resolution_weight: default_res_weight(),
            codec_weight: default_codec_weight(),
            bitrate_weight: default_bitrate_weight(),
            audio_weight: default_audio_weight(),
        }
    }
}
impl Default for CacheConfig {
    fn default() -> Self { Self { path: PathBuf::new(), ttl_days: default_ttl() } }
}
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            scan: ScanConfig::default(),
            ai: AiConfig::default(),
            api: ApiConfig::default(),
            scrape: ScrapeConfig::default(),
            dedup: DedupConfig::default(),
            rename: RenameConfig::default(),
            organize: OrganizeConfig::default(),
            quality: QualityConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

impl AppConfig {
    /// 获取配置文件路径
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("medio")
            .join("config.toml")
    }

    /// 获取缓存目录路径
    pub fn cache_path(&self) -> PathBuf {
        if self.cache.path.as_os_str().is_empty() {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("medio")
                .join("cache.sled")
        } else {
            self.cache.path.clone()
        }
    }

    /// 加载配置，不存在则生成默认配置
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = AppConfig::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}
