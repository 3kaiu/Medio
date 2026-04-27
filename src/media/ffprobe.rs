use crate::core::config::QualityConfig;
use crate::media::probe::{compute_quality_score, resolution_label, MediaProbe};
use crate::models::media::{ProbeSource, QualityInfo};
use serde::Deserialize;
use std::path::Path;

pub struct FfprobeProbe {
    weights: QualityConfig,
}

impl FfprobeProbe {
    pub fn new(weights: QualityConfig) -> Self {
        Self { weights }
    }

    /// Check if ffprobe is available on the system
    pub fn is_available() -> bool {
        which::which("ffprobe").is_ok()
    }
}

impl MediaProbe for FfprobeProbe {
    fn probe(&self, path: &Path) -> Result<QualityInfo, Box<dyn std::error::Error>> {
        let output = std::process::Command::new("ffprobe")
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_streams",
                "-show_format",
            ])
            .arg(path)
            .output()?;

        if !output.status.success() {
            return Err(format!("ffprobe failed for {:?}", path).into());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let ffprobe: FfprobeOutput = serde_json::from_str(&json_str)?;

        let mut info = QualityInfo::new(ProbeSource::Ffprobe);

        for stream in &ffprobe.streams {
            match stream.codec_type.as_deref() {
                Some("video") => {
                    info.width = stream.width;
                    info.height = stream.height;
                    info.video_codec = stream.codec_name.clone();
                    info.video_bitrate = stream.bit_rate.or_else(|| {
                        ffprobe.format.bit_rate.as_ref().and_then(|b| b.parse().ok())
                    });
                    info.duration_secs = ffprobe.format.duration.as_ref().and_then(|d| d.parse().ok());
                }
                Some("audio") => {
                    info.audio_codec = stream.codec_name.clone();
                    info.audio_bitrate = stream.bit_rate;
                }
                _ => {}
            }
        }

        info.resolution_label = resolution_label(info.width, info.height);
        info.quality_score = compute_quality_score(&info, &self.weights);
        Ok(info)
    }
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
    format: FfprobeFormat,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    bit_rate: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
}
