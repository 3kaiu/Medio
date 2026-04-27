use crate::core::config::QualityConfig;
use crate::models::media::QualityInfo;
use std::path::Path;

pub trait MediaProbe: Send + Sync {
    fn probe(&self, path: &Path) -> Result<QualityInfo, Box<dyn std::error::Error>>;
}

/// Compute quality score from raw info and weights
pub fn compute_quality_score(q: &QualityInfo, weights: &QualityConfig) -> f64 {
    let res_score = match q.height {
        Some(h) if h >= 2160 => 100.0,
        Some(h) if h >= 1080 => 80.0,
        Some(h) if h >= 720 => 60.0,
        Some(_) => 40.0,
        None => 50.0,
    };

    let codec_score = match q.video_codec.as_deref() {
        Some("AV1") => 95.0,
        Some("H.265") | Some("HEVC") => 90.0,
        Some("H.264") => 70.0,
        Some(_) => 50.0,
        None => 50.0,
    };

    let bitrate_score = q
        .video_bitrate
        .map(|b| (b as f64 / 50_000_000.0 * 100.0).min(100.0))
        .unwrap_or(50.0);

    let audio_score = match q.audio_codec.as_deref() {
        Some("DTS") | Some("Atmos") => 95.0,
        Some("FLAC") => 90.0,
        Some("AAC") => 65.0,
        Some(_) => 50.0,
        None => 50.0,
    };

    res_score * weights.resolution_weight
        + codec_score * weights.codec_weight
        + bitrate_score * weights.bitrate_weight
        + audio_score * weights.audio_weight
}

/// Get resolution label from height
pub fn resolution_label(_width: Option<u32>, height: Option<u32>) -> String {
    match height {
        Some(h) if h >= 2160 => "4K".into(),
        Some(h) if h >= 1080 => "1080p".into(),
        Some(h) if h >= 720 => "720p".into(),
        Some(h) if h >= 480 => "480p".into(),
        Some(h) => format!("{h}p"),
        None => "unknown".into(),
    }
}
