use crate::core::config::QualityConfig;
use crate::media::probe::{MediaProbe, compute_quality_score, resolution_label};
use crate::models::media::{ProbeSource, QualityInfo};
use std::path::Path;

pub struct NativeProbe {
    weights: QualityConfig,
}

impl NativeProbe {
    pub fn new(weights: QualityConfig) -> Self {
        Self { weights }
    }
}

impl MediaProbe for NativeProbe {
    fn probe(&self, path: &Path) -> Result<QualityInfo, Box<dyn std::error::Error>> {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let mut info = match ext.as_str() {
            "mp4" | "m4a" | "m4v" | "mov" => self
                .probe_mp4(path)
                .unwrap_or_else(|_| QualityInfo::new(ProbeSource::Native)),
            "mkv" => self
                .probe_mkv(path)
                .unwrap_or_else(|_| QualityInfo::new(ProbeSource::Native)),
            "mp3" | "flac" | "ogg" | "wav" | "opus" => self
                .probe_audio(path)
                .unwrap_or_else(|_| QualityInfo::new(ProbeSource::Native)),
            _ => QualityInfo::new(ProbeSource::Native),
        };

        info.resolution_label = resolution_label(info.width, info.height);
        info.quality_score = compute_quality_score(&info, &self.weights);
        Ok(info)
    }
}

impl NativeProbe {
    fn probe_mp4(&self, path: &Path) -> Result<QualityInfo, Box<dyn std::error::Error>> {
        let data = std::fs::read(path)?;
        let ctx = mp4parse::read_mp4(&mut data.as_slice())?;

        let mut info = QualityInfo::new(ProbeSource::Native);

        for track in ctx.tracks {
            match track.track_type {
                mp4parse::TrackType::Video => {
                    if let Some(stsd) = track.stsd {
                        for entry in stsd.descriptions {
                            match entry {
                                mp4parse::SampleEntry::Video(v) => {
                                    info.width = Some(v.width as u32);
                                    info.height = Some(v.height as u32);
                                    info.video_codec = Some(format!("{:?}", v.codec_type));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                mp4parse::TrackType::Audio => {
                    if let Some(stsd) = track.stsd {
                        for entry in stsd.descriptions {
                            match entry {
                                mp4parse::SampleEntry::Audio(a) => {
                                    info.audio_codec = Some(format!("{:?}", a.codec_type));
                                    info.audio_bitrate =
                                        Some(a.samplerate as u64 * a.channelcount as u64 * 16);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(info)
    }

    fn probe_mkv(&self, path: &Path) -> Result<QualityInfo, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(path)?;
        let mkv = matroska_demuxer::MatroskaFile::open(file)?;

        let mut info = QualityInfo::new(ProbeSource::Native);

        for track in mkv.tracks() {
            let codec = track.codec_id().to_string();
            if let Some(vi) = track.video() {
                info.width = Some(vi.pixel_width().get() as u32);
                info.height = Some(vi.pixel_height().get() as u32);
                info.video_codec = Some(map_mkv_codec(&codec));
            } else if track.audio().is_some() {
                info.audio_codec = Some(map_mkv_audio_codec(&codec));
            }
        }

        Ok(info)
    }

    fn probe_audio(&self, path: &Path) -> Result<QualityInfo, Box<dyn std::error::Error>> {
        let mut info = QualityInfo::new(ProbeSource::Native);

        // Basic audio probe using file extension as codec hint
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        info.audio_codec = Some(match ext.as_str() {
            "mp3" => "MP3".into(),
            "flac" => "FLAC".into(),
            "ogg" => "Vorbis".into(),
            "wav" => "PCM".into(),
            "opus" => "Opus".into(),
            "m4a" | "aac" => "AAC".into(),
            _ => "unknown".into(),
        });

        // Try symphonia for duration/bitrate
        if let Ok(file) = std::fs::File::open(path) {
            let mss =
                symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());

            let hint = symphonia::core::probe::Hint::new();
            let meta_opts = symphonia::core::meta::MetadataOptions::default();
            let format_opts = symphonia::core::formats::FormatOptions::default();
            if let Ok(probed) =
                symphonia::default::get_probe().format(&hint, mss, &format_opts, &meta_opts)
            {
                if let Some(track) = probed.format.default_track() {
                    let params = &track.codec_params;
                    info.audio_codec = Some(format!("{:?}", params.codec));
                    info.duration_secs = params
                        .time_base
                        .and_then(|tb| params.n_frames.map(|f| tb.calc_time(f).seconds as u64));
                }
            }
        }

        Ok(info)
    }
}

fn map_mkv_codec(codec: &str) -> String {
    match codec {
        "V_MPEG4/ISO/AVC" => "H.264".into(),
        "V_MPEGH/ISO/HEVC" => "H.265".into(),
        "V_AV1" => "AV1".into(),
        "V_VP9" => "VP9".into(),
        "V_MPEG4/ISO/ASP" => "MPEG-4".into(),
        other => other.into(),
    }
}

fn map_mkv_audio_codec(codec: &str) -> String {
    match codec {
        "A_AAC" => "AAC".into(),
        "A_FLAC" => "FLAC".into(),
        "A_DTS" => "DTS".into(),
        "A_AC3" => "AC3".into(),
        "A_OPUS" => "Opus".into(),
        "A_VORBIS" => "Vorbis".into(),
        other => other.into(),
    }
}
