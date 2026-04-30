use crate::core::config::DedupConfig;
use crate::core::types::KeepStrategy;
use crate::engine::execution_report::ExecutionReport;
use crate::models::media::{HashInfo, MediaItem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub content_id: String,
    pub kind: DuplicateKind,
    pub keep_strategy: String,
    pub summary: String,
    #[serde(default)]
    pub decision: DuplicateDecisionProfile,
    pub guardrails: Vec<String>,
    pub items: Vec<DuplicateItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DuplicateDecisionProfile {
    pub keep_index: Option<usize>,
    pub drop_count: usize,
    pub guarded: bool,
    pub manual_review: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicateItem {
    pub index: usize,
    pub quality_score: f64,
    pub metadata_confidence: f32,
    pub is_keep: bool,
    pub rationale: String,
    pub basis: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateKind {
    Exact,
    Version,
}

pub struct Deduplicator {
    config: DedupConfig,
}

impl Deduplicator {
    pub fn new(config: DedupConfig) -> Self {
        Self { config }
    }

    /// Analyze items for duplicates, return duplicate groups
    pub fn analyze(&self, items: &[MediaItem]) -> Vec<DuplicateGroup> {
        let mut groups: Vec<DuplicateGroup> = Vec::new();

        // 1. Exact dedup: group by full_hash
        let mut hash_groups: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            if let Some(HashInfo {
                full_hash: Some(h), ..
            }) = &item.hash
            {
                hash_groups.entry(*h).or_default().push(i);
            }
        }

        for (hash, indices) in hash_groups {
            if indices.len() < 2 {
                continue;
            }
            groups.push(self.build_group(
                &format!("hash:{hash}"),
                DuplicateKind::Exact,
                &indices,
                items,
            ));
        }

        // 2. Version dedup: group by confirmed identity first, title fallback second
        let mut content_groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            let Some(key) = version_group_key(item) else {
                continue;
            };
            content_groups.entry(key).or_default().push(i);
        }

        for (content_key, indices) in content_groups {
            if indices.len() < 2 {
                continue;
            }
            // Skip if already covered by exact dedup
            let hashes: Vec<Option<u64>> = indices
                .iter()
                .map(|&i| items[i].hash.as_ref().and_then(|h| h.full_hash))
                .collect();
            let all_same = hashes.windows(2).all(|w| w[0].is_some() && w[0] == w[1]);
            if all_same {
                continue;
            }
            groups.push(self.build_group(&content_key, DuplicateKind::Version, &indices, items));
        }

        groups
    }

    /// Execute dedup actions (dry-run supported)
    pub async fn execute(
        &self,
        groups: &[DuplicateGroup],
        items: &[MediaItem],
        dry_run: bool,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(self.execute_report(groups, items, dry_run).await?.details)
    }

    pub async fn execute_report(
        &self,
        groups: &[DuplicateGroup],
        items: &[MediaItem],
        dry_run: bool,
    ) -> Result<ExecutionReport, Box<dyn std::error::Error>> {
        let mut report = ExecutionReport::new("dedup");

        for group in groups {
            if self.config.duplicate_action != crate::core::types::DupAction::Report
                && !group.guardrails.is_empty()
            {
                report.guarded += 1;
                report.details.push(format!("[guard] {}", group.content_id));
                for guard in &group.guardrails {
                    report.details.push(format!("  - {guard}"));
                }
                continue;
            }
            for item in &group.items {
                if item.is_keep {
                    continue;
                }
                let media = &items[item.index];
                let path = &media.path;
                let action_desc = format!(
                    "REMOVE {} (score: {:.1})",
                    path.display(),
                    item.quality_score
                );

                if dry_run {
                    report.executed += 1;
                    report.details.push(format!("[dry-run] {action_desc}"));
                } else {
                    match self.config.duplicate_action {
                        crate::core::types::DupAction::Trash => match trash::delete(path) {
                            Ok(()) => {
                                let msg = format!("[trash] {action_desc}");
                                crate::core::oplog::log(&msg);
                                report.executed += 1;
                                report.details.push(msg);
                            }
                            Err(e) => {
                                report.errors += 1;
                                report.details.push(format!(
                                    "[error] failed to trash {}: {e}",
                                    path.display()
                                ))
                            }
                        },
                        crate::core::types::DupAction::Move => {
                            if self.config.move_target.as_os_str().is_empty() {
                                report.skipped += 1;
                                report.details.push(format!(
                                    "[skip] no move_target configured for {}",
                                    path.display()
                                ));
                            } else {
                                let dest = self
                                    .config
                                    .move_target
                                    .join(path.file_name().unwrap_or_default());
                                match std::fs::rename(path, &dest) {
                                    Ok(()) => {
                                        let msg = format!(
                                            "[move] {} → {}",
                                            path.display(),
                                            dest.display()
                                        );
                                        crate::core::oplog::log(&msg);
                                        report.executed += 1;
                                        report.details.push(msg);
                                    }
                                    Err(e) => {
                                        report.errors += 1;
                                        report.details.push(format!(
                                            "[error] failed to move {}: {e}",
                                            path.display()
                                        ));
                                    }
                                }
                            }
                        }
                        crate::core::types::DupAction::Report => {
                            report.executed += 1;
                            report.details.push(format!("[report] {action_desc}"));
                        }
                    }
                }
            }
        }

        Ok(report)
    }

    fn build_group(
        &self,
        content_id: &str,
        kind: DuplicateKind,
        indices: &[usize],
        items: &[MediaItem],
    ) -> DuplicateGroup {
        let mut dup_items: Vec<DuplicateItem> = indices
            .iter()
            .map(|&i| {
                let score = items[i]
                    .quality
                    .as_ref()
                    .map(|q| q.quality_score)
                    .unwrap_or(0.0);
                DuplicateItem {
                    index: i,
                    quality_score: score,
                    metadata_confidence: items[i].preferred_metadata_confidence(),
                    is_keep: false,
                    rationale: String::new(),
                    basis: duplicate_basis(&items[i]),
                }
            })
            .collect();

        // Determine which to keep
        let keep_idx = match self.config.keep_strategy {
            KeepStrategy::HighestQuality => dup_items
                .iter()
                .enumerate()
                .max_by(|a, b| {
                    effective_keep_score(&items[a.1.index], a.1)
                        .partial_cmp(&effective_keep_score(&items[b.1.index], b.1))
                        .unwrap()
                        .then_with(|| a.1.quality_score.partial_cmp(&b.1.quality_score).unwrap())
                        .then_with(|| {
                            items[a.1.index]
                                .identity_trust_score()
                                .partial_cmp(&items[b.1.index].identity_trust_score())
                                .unwrap()
                        })
                        .then_with(|| {
                            a.1.metadata_confidence
                                .partial_cmp(&b.1.metadata_confidence)
                                .unwrap()
                        })
                        .then_with(|| items[a.1.index].file_size.cmp(&items[b.1.index].file_size))
                })
                .map(|(i, _)| i),
            KeepStrategy::Newest => {
                // Pre-fetch modified times to avoid repeated fs::metadata calls
                let mod_times: Vec<Option<std::time::SystemTime>> = dup_items
                    .iter()
                    .map(|d| {
                        std::fs::metadata(&items[d.index].path)
                            .and_then(|m| m.modified())
                            .ok()
                    })
                    .collect();
                mod_times
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.is_some())
                    .max_by_key(|(_, t)| t.unwrap())
                    .map(|(i, _)| i)
            }
            KeepStrategy::Largest => dup_items
                .iter()
                .enumerate()
                .max_by_key(|(_, d)| items[d.index].file_size)
                .map(|(i, _)| i),
            KeepStrategy::Manual => None,
        };

        if let Some(ki) = keep_idx {
            dup_items[ki].is_keep = true;
            dup_items[ki].rationale = keep_rationale(
                self.config.keep_strategy,
                &dup_items[ki],
                &items[dup_items[ki].index],
            );
        }

        let keep_index = dup_items
            .iter()
            .find(|candidate| candidate.is_keep)
            .map(|item| item.index);
        for item in dup_items.iter_mut().filter(|item| !item.is_keep) {
            item.rationale = remove_rationale(
                kind,
                self.config.keep_strategy,
                item,
                &items[item.index],
                keep_index,
                items,
            );
        }

        let guardrails = group_guardrails(kind, self.config.keep_strategy, &dup_items, items);

        DuplicateGroup {
            content_id: content_id.to_string(),
            kind,
            keep_strategy: format!("{:?}", self.config.keep_strategy),
            summary: group_summary(kind, self.config.keep_strategy, &dup_items, items),
            decision: DuplicateDecisionProfile {
                keep_index,
                drop_count: dup_items.iter().filter(|item| !item.is_keep).count(),
                guarded: !guardrails.is_empty(),
                manual_review: self.config.keep_strategy == KeepStrategy::Manual,
            },
            guardrails,
            items: dup_items,
        }
    }
}

fn duplicate_basis(item: &MediaItem) -> Vec<String> {
    let mut basis = Vec::new();
    basis.push(format!("size={}B", item.file_size));
    basis.push(format!("identity_trust={:.2}", item.identity_trust_score()));
    if let Some(label) = item.identity_confirmation_label() {
        basis.push(format!("identity_state={label}"));
    }
    if let Some(key) = version_group_key(item) {
        basis.push(format!("version_key={key}"));
    }
    basis.push(format!(
        "version_quality={:.2}",
        version_quality_score(item)
    ));
    basis.push(format!("completeness={:.2}", completeness_score(item)));
    if let Some(quality) = &item.quality {
        basis.push(format!("quality={:.1}", quality.quality_score));
        basis.push(format!("resolution={}", quality.resolution_label));
        if let Some(codec) = &quality.video_codec {
            basis.push(format!("video={codec}"));
        }
        if let Some(audio) = &quality.audio_codec {
            basis.push(format!("audio={audio}"));
        }
        if let Some(video_bitrate) = quality.video_bitrate {
            basis.push(format!("video_bitrate={video_bitrate}"));
        }
        if let Some(audio_bitrate) = quality.audio_bitrate {
            basis.push(format!("audio_bitrate={audio_bitrate}"));
        }
        if let Some(duration_secs) = quality.duration_secs {
            basis.push(format!("duration_secs={duration_secs}"));
        }
    }
    if let Some(content) = &item.content_evidence {
        basis.push(format!("subtitle_sources={}", content.subtitles.len()));
        if let Some(runtime_secs) = content.runtime_secs {
            basis.push(format!("content_runtime_secs={runtime_secs}"));
        }
    }
    if let Some(hash) = &item.hash {
        if let Some(full) = hash.full_hash {
            basis.push(format!("full_hash={full:016x}"));
        }
    }
    if let Some(title) = item.preferred_title() {
        basis.push(format!(
            "identity_title={} ({:?}, {:.2})",
            title.value, title.origin, title.confidence
        ));
    }
    if let Some(year) = item.preferred_year() {
        basis.push(format!(
            "identity_year={} ({:?}, {:.2})",
            year.value, year.origin, year.confidence
        ));
    }
    basis
}

fn keep_rationale(strategy: KeepStrategy, entry: &DuplicateItem, item: &MediaItem) -> String {
    match strategy {
        KeepStrategy::HighestQuality => format!(
            "kept as best weighted candidate (quality {:.1}, version {:.2}, completeness {:.2}, metadata {:.2}, identity {:.2}, size {}B)",
            entry.quality_score,
            version_quality_score(item),
            completeness_score(item),
            entry.metadata_confidence,
            item.identity_trust_score(),
            item.file_size
        ),
        KeepStrategy::Newest => "kept as newest modified candidate".into(),
        KeepStrategy::Largest => format!("kept as largest candidate ({}B)", item.file_size),
        KeepStrategy::Manual => "no automatic keep decision; retained for manual review".into(),
    }
}

fn remove_rationale(
    kind: DuplicateKind,
    strategy: KeepStrategy,
    entry: &DuplicateItem,
    item: &MediaItem,
    keep_index: Option<usize>,
    items: &[MediaItem],
) -> String {
    let keep_desc = keep_index
        .map(|winner_index| {
            let winner_item = &items[winner_index];
            winner_item
                .path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| winner_item.path.display().to_string())
        })
        .unwrap_or_else(|| "manual review".into());
    let duplicate_desc = match kind {
        DuplicateKind::Exact => "exact duplicate",
        DuplicateKind::Version => "lower-ranked version duplicate",
    };

    match strategy {
        KeepStrategy::HighestQuality => format!(
            "{duplicate_desc}; lower rank than kept candidate {keep_desc} (quality {:.1}, version {:.2}, completeness {:.2}, metadata {:.2}, identity {:.2}, {}B)",
            entry.quality_score,
            version_quality_score(item),
            completeness_score(item),
            entry.metadata_confidence,
            item.identity_trust_score(),
            item.file_size
        ),
        KeepStrategy::Newest => format!(
            "{duplicate_desc}; older than kept candidate {keep_desc} ({:.1}, {}B)",
            entry.quality_score, item.file_size
        ),
        KeepStrategy::Largest => format!(
            "{duplicate_desc}; smaller than kept candidate {keep_desc} ({}B)",
            item.file_size
        ),
        KeepStrategy::Manual => {
            format!("{duplicate_desc}; flagged for manual review against {keep_desc}")
        }
    }
}

fn group_summary(
    kind: DuplicateKind,
    strategy: KeepStrategy,
    dup_items: &[DuplicateItem],
    items: &[MediaItem],
) -> String {
    let keep_name = dup_items
        .iter()
        .find(|item| item.is_keep)
        .map(|item| {
            items[item.index]
                .path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| items[item.index].path.display().to_string())
        })
        .unwrap_or_else(|| "manual review".into());
    format!(
        "{:?} duplicate group, strategy {:?}, keep {}",
        kind, strategy, keep_name
    )
}

fn group_guardrails(
    kind: DuplicateKind,
    strategy: KeepStrategy,
    dup_items: &[DuplicateItem],
    items: &[MediaItem],
) -> Vec<String> {
    let mut guards = Vec::new();
    let Some(keep_item) = dup_items.iter().find(|item| item.is_keep) else {
        guards.push("no automatic keep winner selected".into());
        return guards;
    };

    if strategy == KeepStrategy::Manual {
        guards.push("manual keep strategy requires human review before destructive dedup".into());
    }

    if kind == DuplicateKind::Version {
        let fallback_group = dup_items.iter().all(|item| {
            version_group_key(&items[item.index])
                .map(|key| key.starts_with("fallback:"))
                .unwrap_or(false)
        });
        if fallback_group {
            guards.push(
                "version grouping relies on fallback title identity; review before destructive dedup"
                    .into(),
            );
        }
        if keep_item.metadata_confidence < 0.78 {
            guards.push(format!(
                "trusted identity too weak for version dedup automation (winner confidence {:.2})",
                keep_item.metadata_confidence
            ));
        }
        let keep_identity = items[keep_item.index].identity_trust_score();
        let strongest_removed_identity = dup_items
            .iter()
            .filter(|item| !item.is_keep)
            .map(|item| items[item.index].identity_trust_score())
            .fold(0.0, f32::max);
        if strongest_removed_identity > keep_identity + 0.12 {
            guards.push(format!(
                "removed candidate has materially stronger confirmed identity ({strongest_removed_identity:.2} > {keep_identity:.2})"
            ));
        }
        let max_remove_score = dup_items
            .iter()
            .filter(|item| !item.is_keep)
            .map(|item| item.quality_score)
            .fold(0.0, f64::max);
        if (keep_item.quality_score - max_remove_score).abs() < 0.25 {
            guards.push("quality delta between kept and removed versions is too small".into());
        }
        if completeness_score(&items[keep_item.index]) < 0.0 {
            guards.push("kept candidate appears incomplete or sample-like".into());
        }
        let strongest_removed_completeness = dup_items
            .iter()
            .filter(|item| !item.is_keep)
            .map(|item| completeness_score(&items[item.index]))
            .fold(f64::NEG_INFINITY, f64::max);
        if strongest_removed_completeness > completeness_score(&items[keep_item.index]) + 0.45 {
            guards.push(format!(
                "removed candidate appears materially more complete ({strongest_removed_completeness:.2} > {:.2})",
                completeness_score(&items[keep_item.index])
            ));
        }
        let weakest_removed_completeness = dup_items
            .iter()
            .filter(|item| !item.is_keep)
            .map(|item| completeness_score(&items[item.index]))
            .fold(f64::INFINITY, f64::min);
        if weakest_removed_completeness < -0.5 {
            guards.push(format!(
                "one or more removed candidates appear incomplete or sample-like ({weakest_removed_completeness:.2})"
            ));
        }
    }

    guards
}

fn effective_keep_score(item: &MediaItem, entry: &DuplicateItem) -> f64 {
    entry.quality_score
        + version_quality_score(item)
        + completeness_score(item)
        + f64::from(entry.metadata_confidence) * 0.35
        + f64::from(item.identity_trust_score()) * 0.45
}

fn version_quality_score(item: &MediaItem) -> f64 {
    let mut score = 0.0;

    if let Some(quality) = &item.quality {
        if let Some(video_bitrate) = quality.video_bitrate {
            score += (video_bitrate as f64 / 4_000_000.0).min(1.1);
        }
        if let Some(audio_bitrate) = quality.audio_bitrate {
            score += (audio_bitrate as f64 / 512_000.0).min(0.35);
        }
        if let Some(duration_secs) = quality.duration_secs
            && duration_secs >= 1_200
        {
            score += 0.18;
        }
        score += resolution_bonus(&quality.resolution_label);
        score += codec_bonus(
            quality.video_codec.as_deref(),
            quality.audio_codec.as_deref(),
        );
    }

    if let Some(parsed) = &item.parsed {
        score += media_suffix_bonus(parsed.media_suffix.as_deref());
        score += media_suffix_bonus(parsed.source.as_deref());
        score += media_suffix_bonus(parsed.resolution.as_deref());
        score += release_tier_bonus(parsed.media_suffix.as_deref());
        score += release_tier_bonus(parsed.source.as_deref());
    }

    if let Some(content) = &item.content_evidence {
        if !content.subtitles.is_empty() {
            score += 0.22 + (content.subtitles.len().min(3) as f64 - 1.0).max(0.0) * 0.05;
        }
        if content.subtitles.iter().any(|sub| {
            sub.language
                .as_deref()
                .is_some_and(is_preferred_subtitle_language)
        }) {
            score += 0.08;
        }
        if let (Some(content_runtime), Some(quality_runtime)) = (
            content.runtime_secs,
            item.quality
                .as_ref()
                .and_then(|quality| quality.duration_secs),
        ) {
            let delta = content_runtime.abs_diff(quality_runtime);
            if delta <= 3 {
                score += 0.08;
            } else if delta >= 90 {
                score -= 0.18;
            }
        }
        score += presentation_depth_bonus(content);
    }

    score
}

fn completeness_score(item: &MediaItem) -> f64 {
    let mut score = 0.0;

    let path_lower = item.path.to_string_lossy().to_ascii_lowercase();
    if is_sample_like_path(&path_lower) {
        score -= 1.25;
    }
    if is_extras_like_path(&path_lower) {
        score -= 0.85;
    }

    if let Some(content) = &item.content_evidence {
        let forced_only = !content.subtitles.is_empty()
            && content.subtitles.iter().all(|sub| {
                sub.track_title
                    .as_deref()
                    .map(is_forced_track_title)
                    .unwrap_or(false)
            });
        if forced_only {
            score -= 0.55;
        } else if !content.subtitles.is_empty() {
            score += 0.18;
        }

        if content
            .risk_flags
            .iter()
            .any(|flag| flag.to_ascii_lowercase().contains("failed"))
        {
            score -= 0.15;
        }

        if let (Some(content_runtime), Some(quality_runtime)) = (
            content.runtime_secs,
            item.quality
                .as_ref()
                .and_then(|quality| quality.duration_secs),
        ) {
            let delta = content_runtime.abs_diff(quality_runtime);
            if delta >= 180 {
                score -= 0.4;
            }
        }
    }

    if let Some(quality) = &item.quality {
        if let Some(duration_secs) = quality.duration_secs {
            match item.media_type {
                crate::models::media::MediaType::Movie if duration_secs < 2_400 => score -= 0.75,
                crate::models::media::MediaType::TvShow | crate::models::media::MediaType::Strm
                    if duration_secs < 600 =>
                {
                    score -= 0.55
                }
                _ => {}
            }
        }
    }

    score += episode_provenance_bonus(item);

    score
}

fn resolution_bonus(label: &str) -> f64 {
    match label.trim().to_ascii_lowercase().as_str() {
        "2160p" | "4k" => 0.65,
        "1440p" => 0.4,
        "1080p" => 0.24,
        "720p" => 0.08,
        _ => 0.0,
    }
}

fn codec_bonus(video_codec: Option<&str>, audio_codec: Option<&str>) -> f64 {
    let mut score = 0.0;
    let video = video_codec.unwrap_or_default().to_ascii_lowercase();
    if video.contains("265") || video.contains("hevc") {
        score += 0.12;
    }
    let audio = audio_codec.unwrap_or_default().to_ascii_lowercase();
    if audio.contains("truehd") || audio.contains("dts-hd") || audio.contains("atmos") {
        score += 0.2;
    } else if audio.contains("dts") {
        score += 0.1;
    }
    score
}

fn media_suffix_bonus(raw: Option<&str>) -> f64 {
    let lower = raw.unwrap_or_default().to_ascii_lowercase();
    let mut score = 0.0;
    if lower.contains("hdr") {
        score += 0.28;
    }
    if lower.contains("dv") || lower.contains("dovi") || lower.contains("dolby vision") {
        score += 0.3;
    }
    if lower.contains("remux") {
        score += 0.36;
    }
    if lower.contains("atmos") {
        score += 0.2;
    }
    if lower.contains("truehd") || lower.contains("dts-hd") {
        score += 0.16;
    }
    score
}

fn release_tier_bonus(raw: Option<&str>) -> f64 {
    let lower = raw.unwrap_or_default().to_ascii_lowercase();
    if lower.is_empty() {
        return 0.0;
    }
    if lower.contains("remux") || lower.contains("uhd blu") || lower.contains("ultra hd blu") {
        return 0.48;
    }
    if lower.contains("bluray") || lower.contains("blu-ray") || lower.contains("bdrip") {
        return 0.24;
    }
    if lower.contains("web-dl") || lower.contains("webdl") {
        return 0.12;
    }
    if lower.contains("webrip") {
        return 0.06;
    }
    if lower.contains("hdtv") {
        return -0.04;
    }
    if lower.contains("cam") || lower.contains("ts") || lower.contains("tc") {
        return -0.45;
    }
    0.0
}

fn presentation_depth_bonus(content: &crate::models::media::ContentEvidence) -> f64 {
    let mut score = 0.0;

    let commentary_tracks = content
        .container
        .track_titles
        .iter()
        .filter(|title| is_commentary_track_title(title))
        .count();
    let presentation_tracks = content
        .container
        .track_titles
        .len()
        .saturating_sub(commentary_tracks);
    if presentation_tracks >= 2 {
        score += 0.16;
    } else if presentation_tracks == 1 {
        score += 0.06;
    }

    if content.container.stream_languages.len() >= 2 {
        score += 0.08;
    }

    let full_dialog_subtitles = content
        .subtitles
        .iter()
        .filter(|sub| {
            !sub.track_title
                .as_deref()
                .map(is_forced_track_title)
                .unwrap_or(false)
                && !sub
                    .track_title
                    .as_deref()
                    .map(is_commentary_track_title)
                    .unwrap_or(false)
        })
        .count();
    if full_dialog_subtitles >= 2 {
        score += 0.12;
    }

    score
}

fn is_preferred_subtitle_language(language: &str) -> bool {
    matches!(
        language.trim().to_ascii_lowercase().as_str(),
        "eng" | "en" | "chi" | "zho" | "zh" | "chs" | "cht"
    )
}

fn is_forced_track_title(track_title: &str) -> bool {
    let lower = track_title.trim().to_ascii_lowercase();
    lower.contains("forced") && !lower.contains("full")
}

fn is_commentary_track_title(track_title: &str) -> bool {
    track_title
        .trim()
        .to_ascii_lowercase()
        .contains("commentary")
}

fn version_group_key(item: &MediaItem) -> Option<String> {
    let season = item
        .preferred_season()
        .map(|decision| decision.value)
        .unwrap_or(0);
    let episode = item
        .preferred_episode()
        .map(|decision| decision.value)
        .unwrap_or(0);

    if item.identity_trust_score() >= 0.82
        && let Some(scraped) = &item.scraped
    {
        if let Some(tmdb_id) = scraped.tmdb_id {
            return Some(match item.media_type {
                crate::models::media::MediaType::TvShow | crate::models::media::MediaType::Strm => {
                    format!(
                        "tmdb:tv:{tmdb_id}:S{season:02}E{episode:02}:{}",
                        episode_provenance_tag(item)
                    )
                }
                _ => {
                    let year = scraped
                        .year
                        .or_else(|| item.preferred_year().map(|decision| decision.value))
                        .unwrap_or(0);
                    format!("tmdb:movie:{tmdb_id}:{year}")
                }
            });
        }
        if let Some(id) = &scraped.musicbrainz_id {
            return Some(format!("musicbrainz:{id}"));
        }
        if let Some(id) = &scraped.openlibrary_id {
            return Some(format!("openlibrary:{id}"));
        }
    }

    let title = item.preferred_title()?.value;
    let year = item
        .preferred_year()
        .map(|decision| decision.value)
        .unwrap_or(0);
    Some(format!(
        "fallback:{title}|{year}|S{season:02}E{episode:02}:{}",
        episode_provenance_tag(item)
    ))
}

fn episode_provenance_tag(item: &MediaItem) -> &'static str {
    if !matches!(
        item.media_type,
        crate::models::media::MediaType::TvShow | crate::models::media::MediaType::Strm
    ) {
        return "feature";
    }

    let path_lower = item.path.to_string_lossy().to_ascii_lowercase();
    if is_extras_like_path(&path_lower) {
        "extras"
    } else if has_episode_cluster_siblings(item) {
        "season_cluster"
    } else if looks_like_season_pack_episode_path(&path_lower) {
        "season"
    } else {
        "loose"
    }
}

fn episode_provenance_bonus(item: &MediaItem) -> f64 {
    match episode_provenance_tag(item) {
        "season_cluster" => 0.14,
        "season" => 0.08,
        "loose" => 0.02,
        "extras" => -0.65,
        _ => 0.0,
    }
}

fn has_episode_cluster_siblings(item: &MediaItem) -> bool {
    let Some(parent) = item.path.parent() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return false;
    };

    let mut episode_like = 0usize;
    let mut media_like = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        else {
            continue;
        };
        if !matches!(
            ext.as_str(),
            "mkv" | "mp4" | "avi" | "mov" | "m4v" | "ts" | "m2ts" | "wmv" | "webm"
        ) {
            continue;
        }
        media_like += 1;
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        if stem_contains_episode_marker(&stem) || stem_is_episode_number(&stem) {
            episode_like += 1;
        }
    }

    media_like >= 3 && episode_like >= 2
}

fn looks_like_season_pack_episode_path(path_lower: &str) -> bool {
    path_lower.contains("/season ")
        || path_lower.contains("/s01")
        || path_lower.contains("/s02")
        || path_lower.contains("/s03")
        || path_lower.contains("/s04")
        || path_lower.contains("/s1/")
        || path_lower.contains("/s2/")
}

fn stem_contains_episode_marker(stem: &str) -> bool {
    let stem = stem.trim();
    let upper = stem.to_ascii_uppercase();
    upper.contains("S01E")
        || upper.contains("S02E")
        || upper.contains("S03E")
        || upper.contains("S04E")
        || upper.contains("EP")
}

fn stem_is_episode_number(stem: &str) -> bool {
    let trimmed = stem.trim();
    !trimmed.is_empty() && trimmed.len() <= 3 && trimmed.chars().all(|ch| ch.is_ascii_digit())
}

fn is_extras_like_path(path_lower: &str) -> bool {
    path_lower.contains("/extras/")
        || path_lower.contains("/featurettes/")
        || path_lower.contains("/behind the scenes/")
        || path_lower.contains("/specials/")
        || path_lower.contains("特别篇")
        || path_lower.contains("花絮")
}

fn is_sample_like_path(path_lower: &str) -> bool {
    path_lower.contains("/sample/")
        || path_lower.contains(" sample")
        || path_lower.contains("-sample")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::DedupConfig;
    use crate::core::types::{DupAction, KeepStrategy};
    use crate::models::media::{
        ConfirmationState, IdentityResolution, MediaItem, MediaType, ParseSource, ParsedInfo,
        QualityInfo, ScrapeResult, ScrapeSource,
    };
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    fn make_item(path: PathBuf, full_hash: u64) -> MediaItem {
        MediaItem {
            id: 0,
            path,
            file_size: 1024,
            media_type: MediaType::Movie,
            extension: "mkv".into(),
            parsed: None,
            quality: None,
            scraped: None,
            content_evidence: None,
            identity_resolution: None,
            hash: Some(HashInfo {
                size_hash: full_hash,
                prefix_hash: Some(full_hash),
                full_hash: Some(full_hash),
            }),
            rename_plan: None,
        }
    }

    fn with_identity_state(mut item: MediaItem, state: ConfirmationState) -> MediaItem {
        item.identity_resolution = Some(IdentityResolution {
            confirmation_state: state,
            best: None,
            candidates: Vec::new(),
            evidence_refs: Vec::new(),
            risk_flags: Vec::new(),
        });
        item
    }

    fn with_quality(mut item: MediaItem, score: f64) -> MediaItem {
        item.quality = Some(QualityInfo {
            width: Some(1920),
            height: Some(1080),
            resolution_label: "1080p".into(),
            video_codec: Some("h264".into()),
            video_bitrate: None,
            audio_codec: Some("aac".into()),
            audio_bitrate: None,
            duration_secs: None,
            quality_score: score,
            probe_source: crate::models::media::ProbeSource::Native,
        });
        item
    }

    fn with_quality_profile(
        mut item: MediaItem,
        resolution_label: &str,
        video_bitrate: Option<u64>,
        audio_codec: Option<&str>,
        audio_bitrate: Option<u64>,
        duration_secs: Option<u64>,
        quality_score: f64,
    ) -> MediaItem {
        item.quality = Some(QualityInfo {
            width: Some(1920),
            height: Some(1080),
            resolution_label: resolution_label.into(),
            video_codec: Some("hevc".into()),
            video_bitrate,
            audio_codec: audio_codec.map(|value| value.into()),
            audio_bitrate,
            duration_secs,
            quality_score,
            probe_source: crate::models::media::ProbeSource::Native,
        });
        item
    }

    fn with_tmdb_episode(
        mut item: MediaItem,
        title: &str,
        tmdb_id: u64,
        season: u32,
        episode: u32,
    ) -> MediaItem {
        item.media_type = MediaType::TvShow;
        item.scraped = Some({
            let mut result = ScrapeResult::empty(ScrapeSource::Tmdb, title)
                .with_confidence(0.92)
                .with_evidence(["tmdb episode fixture"]);
            result.tmdb_id = Some(tmdb_id);
            result.season_number = Some(season);
            result.episode_number = Some(episode);
            result
        });
        item
    }

    #[test]
    fn test_keep_strategy_newest_prefers_newer_file() {
        let dir = tempfile::tempdir().unwrap();
        let older_path = dir.path().join("older.mkv");
        let newer_path = dir.path().join("newer.mkv");

        let mut older = std::fs::File::create(&older_path).unwrap();
        writeln!(older, "older").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let mut newer = std::fs::File::create(&newer_path).unwrap();
        writeln!(newer, "newer").unwrap();

        let items = vec![make_item(older_path, 42), make_item(newer_path, 42)];
        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::Newest,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&items);
        assert_eq!(groups.len(), 1);
        let keep_index = groups[0]
            .items
            .iter()
            .find(|item| item.is_keep)
            .map(|item| item.index)
            .unwrap();
        assert_eq!(groups[0].decision.keep_index, Some(keep_index));
        assert_eq!(items[keep_index].path.file_name().unwrap(), "newer.mkv");
        assert_eq!(groups[0].kind, DuplicateKind::Exact);
        assert!(groups[0].summary.contains("keep newer.mkv"));
    }

    #[test]
    fn test_exact_duplicate_rationale_is_populated() {
        let items = vec![
            make_item(PathBuf::from("/tmp/a.mkv"), 42),
            make_item(PathBuf::from("/tmp/b.mkv"), 42),
        ];
        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::Largest,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, DuplicateKind::Exact);
        assert!(!groups[0].items[0].basis.is_empty());
        assert!(
            groups[0]
                .items
                .iter()
                .any(|item| !item.rationale.is_empty())
        );
    }

    #[test]
    fn test_version_group_uses_trusted_identity_over_low_confidence_guess() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.parsed = Some(ParsedInfo {
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
            confidence: 0.92,
            evidence: vec!["test parsed".into()],
        });
        b.parsed = a.parsed.clone();
        a.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Guess, "Noise Cut")
                .with_confidence(0.45)
                .with_evidence(["low confidence guess"]),
        );
        b.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Guess, "Other Noise")
                .with_confidence(0.45)
                .with_evidence(["low confidence guess"]),
        );

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, DuplicateKind::Version);
        assert!(groups[0].content_id.contains("Inception"));
        assert!(
            groups[0]
                .items
                .iter()
                .all(|item| item.metadata_confidence >= 0.9)
        );
    }

    #[test]
    fn test_execute_blocks_destructive_version_dedup_when_guarded() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.parsed = Some(ParsedInfo {
            raw_title: "Same".into(),
            year: Some(2024),
            season: None,
            episode: None,
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.6,
            evidence: vec!["weak parsed".into()],
        });
        b.parsed = a.parsed.clone();
        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Trash,
            move_target: PathBuf::new(),
        });
        let groups = deduplicator.analyze(&[a.clone(), b.clone()]);
        assert_eq!(groups.len(), 1);
        assert!(!groups[0].guardrails.is_empty());
        assert!(groups[0].decision.guarded);

        let rt = crate::core::runtime::build().unwrap();
        let actions = rt
            .block_on(deduplicator.execute(&groups, &[a, b], true))
            .unwrap();
        assert!(actions.iter().any(|line| line.starts_with("[guard]")));
        assert!(!actions.iter().any(|line| line.contains("REMOVE")));
    }

    #[test]
    fn test_highest_quality_strategy_prefers_confirmed_identity_when_scores_are_close() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a = with_quality(a, 8.0);
        b = with_quality(b, 8.2);
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::InsufficientEvidence);
        a.parsed = Some(ParsedInfo {
            raw_title: "Severance".into(),
            year: Some(2022),
            season: Some(1),
            episode: Some(1),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.72,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();
        a.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Tmdb, "Severance")
                .with_confidence(0.92)
                .with_evidence(["confirmed scrape"]),
        );
        b.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Tmdb, "Severance")
                .with_confidence(0.92)
                .with_evidence(["weak scrape"]),
        );

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        let keep = groups[0].items.iter().find(|item| item.is_keep).unwrap();
        assert_eq!(keep.index, 0);
        assert!(keep.rationale.contains("identity 1.00"));
    }

    #[test]
    fn test_version_group_guarded_when_removed_candidate_has_stronger_identity() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a = with_quality(a, 8.6);
        b = with_quality(b, 8.1);
        a = with_identity_state(a, ConfirmationState::InsufficientEvidence);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a.parsed = Some(ParsedInfo {
            raw_title: "Andor".into(),
            year: Some(2022),
            season: Some(1),
            episode: Some(3),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.68,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Trash,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert!(
            groups[0]
                .guardrails
                .iter()
                .any(|line| line.contains("stronger confirmed identity"))
        );
    }

    #[test]
    fn test_version_group_uses_confirmed_tmdb_episode_identity_key() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.parsed = Some(ParsedInfo {
            raw_title: "garbled.one".into(),
            year: Some(2022),
            season: Some(1),
            episode: Some(4),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.45,
            evidence: vec![],
        });
        b.parsed = Some(ParsedInfo {
            raw_title: "noise.two".into(),
            ..a.parsed.clone().unwrap()
        });
        a = with_identity_state(
            with_tmdb_episode(a, "Slow Horses", 197239, 1, 4),
            ConfirmationState::Confirmed,
        );
        b = with_identity_state(
            with_tmdb_episode(b, "Slow Horses", 197239, 1, 4),
            ConfirmationState::Confirmed,
        );

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, DuplicateKind::Version);
        assert_eq!(groups[0].content_id, "tmdb:tv:197239:S01E04:loose");
    }

    #[test]
    fn test_fallback_version_group_is_guarded_for_destructive_dedup() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.parsed = Some(ParsedInfo {
            raw_title: "Unknown.Show".into(),
            year: Some(2023),
            season: Some(1),
            episode: Some(2),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.62,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Trash,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert!(
            groups[0]
                .guardrails
                .iter()
                .any(|line| line.contains("fallback title identity"))
        );
    }

    #[test]
    fn test_version_quality_prefers_hdr_bitrate_and_subtitle_complete_release() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::Movie;
        b.media_type = MediaType::Movie;
        a.parsed = Some(ParsedInfo {
            raw_title: "Dune Part Two".into(),
            year: Some(2024),
            season: None,
            episode: None,
            resolution: Some("2160p HDR DV REMUX".into()),
            codec: Some("hevc".into()),
            source: Some("BluRay Atmos".into()),
            release_group: None,
            media_suffix: Some("2160p.HDR.DV.REMUX.Atmos".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.7,
            evidence: vec![],
        });
        b.parsed = Some(ParsedInfo {
            raw_title: "Dune Part Two".into(),
            year: Some(2024),
            season: None,
            episode: None,
            resolution: Some("1080p".into()),
            codec: Some("hevc".into()),
            source: Some("WEB-DL".into()),
            release_group: None,
            media_suffix: Some("1080p.WEB-DL".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.7,
            evidence: vec![],
        });
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Tmdb, "Dune: Part Two")
                .with_confidence(0.94)
                .with_evidence(["movie fixture"]),
        );
        b.scraped = a.scraped.clone();
        a = with_quality_profile(
            a,
            "2160p",
            Some(28_000_000),
            Some("truehd atmos"),
            Some(768_000),
            Some(9960),
            8.9,
        );
        b = with_quality_profile(
            b,
            "1080p",
            Some(7_000_000),
            Some("aac"),
            Some(192_000),
            Some(9960),
            9.2,
        );
        a.content_evidence = Some(crate::models::media::ContentEvidence {
            subtitles: vec![
                crate::models::media::SubtitleEvidence {
                    source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                    locator: "embedded:stream:2".into(),
                    language: Some("eng".into()),
                    track_title: None,
                    sample_lines: vec![],
                    title_candidates: vec![],
                    season: None,
                    episode: None,
                },
                crate::models::media::SubtitleEvidence {
                    source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                    locator: "embedded:stream:3".into(),
                    language: Some("chi".into()),
                    track_title: None,
                    sample_lines: vec![],
                    title_candidates: vec![],
                    season: None,
                    episode: None,
                },
            ],
            runtime_secs: Some(9960),
            ..Default::default()
        });
        b.content_evidence = Some(crate::models::media::ContentEvidence {
            subtitles: vec![],
            runtime_secs: Some(9960),
            ..Default::default()
        });

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        let keep = groups[0].items.iter().find(|item| item.is_keep).unwrap();
        assert_eq!(keep.index, 0);
        assert!(keep.rationale.contains("version"));
    }

    #[test]
    fn test_completeness_penalizes_sample_like_candidate() {
        let mut a = make_item(PathBuf::from("/library/Movies/sample/Demo.mkv"), 41);
        let mut b = make_item(PathBuf::from("/library/Movies/Feature/Demo.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::Movie;
        b.media_type = MediaType::Movie;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a.parsed = Some(ParsedInfo {
            raw_title: "Demo".into(),
            year: Some(2024),
            season: None,
            episode: None,
            resolution: Some("2160p REMUX".into()),
            codec: Some("hevc".into()),
            source: None,
            release_group: None,
            media_suffix: Some("2160p.REMUX".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.8,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();
        a.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Tmdb, "Demo")
                .with_confidence(0.94)
                .with_evidence(["movie fixture"]),
        );
        b.scraped = a.scraped.clone();
        a = with_quality_profile(
            a,
            "2160p",
            Some(30_000_000),
            Some("truehd"),
            Some(640_000),
            Some(480),
            9.5,
        );
        b = with_quality_profile(
            b,
            "1080p",
            Some(8_000_000),
            Some("aac"),
            Some(192_000),
            Some(7200),
            8.8,
        );

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        let keep = groups[0].items.iter().find(|item| item.is_keep).unwrap();
        assert_eq!(keep.index, 1);
    }

    #[test]
    fn test_completeness_penalizes_forced_only_subtitle_and_short_runtime() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::TvShow;
        b.media_type = MediaType::TvShow;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a.parsed = Some(ParsedInfo {
            raw_title: "Foundation".into(),
            year: Some(2023),
            season: Some(2),
            episode: Some(6),
            resolution: Some("2160p HDR".into()),
            codec: Some("hevc".into()),
            source: None,
            release_group: None,
            media_suffix: Some("2160p.HDR".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.8,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();
        a = with_tmdb_episode(a, "Foundation", 93740, 2, 6);
        b = with_tmdb_episode(b, "Foundation", 93740, 2, 6);
        a = with_quality_profile(
            a,
            "2160p",
            Some(20_000_000),
            Some("dts"),
            Some(512_000),
            Some(420),
            9.3,
        );
        b = with_quality_profile(
            b,
            "1080p",
            Some(7_000_000),
            Some("aac"),
            Some(192_000),
            Some(3300),
            8.4,
        );
        a.content_evidence = Some(crate::models::media::ContentEvidence {
            subtitles: vec![crate::models::media::SubtitleEvidence {
                source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                locator: "embedded:stream:4".into(),
                language: Some("eng".into()),
                track_title: Some("English Forced".into()),
                sample_lines: vec![],
                title_candidates: vec![],
                season: None,
                episode: None,
            }],
            runtime_secs: Some(420),
            ..Default::default()
        });
        b.content_evidence = Some(crate::models::media::ContentEvidence {
            subtitles: vec![crate::models::media::SubtitleEvidence {
                source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                locator: "embedded:stream:2".into(),
                language: Some("eng".into()),
                track_title: Some("English Full".into()),
                sample_lines: vec![],
                title_candidates: vec![],
                season: None,
                episode: None,
            }],
            runtime_secs: Some(3300),
            ..Default::default()
        });

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Trash,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        let keep = groups[0].items.iter().find(|item| item.is_keep).unwrap();
        assert_eq!(keep.index, 1);
        assert!(
            groups[0]
                .guardrails
                .iter()
                .any(|line| line.contains("incomplete or sample-like"))
                || groups[0]
                    .guardrails
                    .iter()
                    .any(|line| line.contains("more complete"))
        );
    }

    #[test]
    fn test_release_tier_prefers_remux_over_webdl_when_identity_matches() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::Movie;
        b.media_type = MediaType::Movie;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a.parsed = Some(ParsedInfo {
            raw_title: "Alien Romulus".into(),
            year: Some(2024),
            season: None,
            episode: None,
            resolution: Some("2160p".into()),
            codec: Some("hevc".into()),
            source: Some("REMUX".into()),
            release_group: None,
            media_suffix: Some("2160p.REMUX.TrueHD.Atmos".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.76,
            evidence: vec![],
        });
        b.parsed = Some(ParsedInfo {
            raw_title: "Alien Romulus".into(),
            year: Some(2024),
            season: None,
            episode: None,
            resolution: Some("2160p".into()),
            codec: Some("hevc".into()),
            source: Some("WEB-DL".into()),
            release_group: None,
            media_suffix: Some("2160p.WEB-DL".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.76,
            evidence: vec![],
        });
        a.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Tmdb, "Alien: Romulus")
                .with_confidence(0.94)
                .with_evidence(["movie fixture"]),
        );
        b.scraped = a.scraped.clone();
        a = with_quality_profile(
            a,
            "2160p",
            Some(24_000_000),
            Some("truehd atmos"),
            Some(640_000),
            Some(7140),
            8.7,
        );
        b = with_quality_profile(
            b,
            "2160p",
            Some(17_000_000),
            Some("aac"),
            Some(256_000),
            Some(7140),
            8.9,
        );

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        let keep = groups[0].items.iter().find(|item| item.is_keep).unwrap();
        assert_eq!(keep.index, 0);
    }

    #[test]
    fn test_presentation_depth_prefers_multi_track_non_commentary_release() {
        let mut a = make_item(PathBuf::from("/tmp/a.mkv"), 41);
        let mut b = make_item(PathBuf::from("/tmp/b.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::TvShow;
        b.media_type = MediaType::TvShow;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a.parsed = Some(ParsedInfo {
            raw_title: "Silo".into(),
            year: Some(2023),
            season: Some(1),
            episode: Some(5),
            resolution: Some("1080p".into()),
            codec: Some("hevc".into()),
            source: Some("WEB-DL".into()),
            release_group: None,
            media_suffix: Some("1080p.WEB-DL".into()),
            parse_source: ParseSource::Regex,
            confidence: 0.75,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();
        a = with_tmdb_episode(a, "Silo", 125988, 1, 5);
        b = with_tmdb_episode(b, "Silo", 125988, 1, 5);
        a = with_quality_profile(
            a,
            "1080p",
            Some(8_000_000),
            Some("aac"),
            Some(256_000),
            Some(3200),
            8.3,
        );
        b = with_quality_profile(
            b,
            "1080p",
            Some(8_400_000),
            Some("aac"),
            Some(256_000),
            Some(3200),
            8.35,
        );
        a.content_evidence = Some(crate::models::media::ContentEvidence {
            container: crate::models::media::ContainerEvidence {
                stream_languages: vec!["eng".into(), "jpn".into()],
                track_titles: vec!["English Full".into(), "Japanese Dub".into()],
                ..Default::default()
            },
            subtitles: vec![
                crate::models::media::SubtitleEvidence {
                    source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                    locator: "embedded:stream:5".into(),
                    language: Some("eng".into()),
                    track_title: Some("English Full".into()),
                    sample_lines: vec![],
                    title_candidates: vec![],
                    season: None,
                    episode: None,
                },
                crate::models::media::SubtitleEvidence {
                    source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                    locator: "embedded:stream:6".into(),
                    language: Some("chi".into()),
                    track_title: Some("Chinese Full".into()),
                    sample_lines: vec![],
                    title_candidates: vec![],
                    season: None,
                    episode: None,
                },
            ],
            runtime_secs: Some(3200),
            ..Default::default()
        });
        b.content_evidence = Some(crate::models::media::ContentEvidence {
            container: crate::models::media::ContainerEvidence {
                stream_languages: vec!["eng".into()],
                track_titles: vec!["Commentary".into()],
                ..Default::default()
            },
            subtitles: vec![crate::models::media::SubtitleEvidence {
                source: crate::models::media::SubtitleEvidenceSource::EmbeddedTrack,
                locator: "embedded:stream:7".into(),
                language: Some("eng".into()),
                track_title: Some("English Commentary".into()),
                sample_lines: vec![],
                title_candidates: vec![],
                season: None,
                episode: None,
            }],
            runtime_secs: Some(3200),
            ..Default::default()
        });

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        let keep = groups[0].items.iter().find(|item| item.is_keep).unwrap();
        assert_eq!(keep.index, 0);
    }

    #[test]
    fn test_extras_episode_does_not_group_with_mainline_episode() {
        let mut a = make_item(PathBuf::from("/library/Show/Season 01/Show.S01E03.mkv"), 41);
        let mut b = make_item(
            PathBuf::from("/library/Show/Specials/Show.S01E03.Special.mkv"),
            42,
        );
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::TvShow;
        b.media_type = MediaType::TvShow;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a = with_tmdb_episode(a, "Shogun", 126308, 1, 3);
        b = with_tmdb_episode(b, "Shogun", 126308, 1, 3);
        a.parsed = Some(ParsedInfo {
            raw_title: "Shogun".into(),
            year: Some(2024),
            season: Some(1),
            episode: Some(3),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.8,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_season_path_episode_groups_as_season_provenance() {
        let mut a = make_item(PathBuf::from("/library/Show/Season 01/01.mkv"), 41);
        let mut b = make_item(PathBuf::from("/library/Show/S01/Episode03.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::TvShow;
        b.media_type = MediaType::TvShow;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a = with_tmdb_episode(a, "The Bear", 209867, 1, 3);
        b = with_tmdb_episode(b, "The Bear", 209867, 1, 3);
        a.parsed = Some(ParsedInfo {
            raw_title: "noise".into(),
            year: Some(2022),
            season: Some(1),
            episode: Some(3),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.5,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].content_id, "tmdb:tv:209867:S01E03:season");
    }

    #[test]
    fn test_numeric_episode_cluster_is_detected_as_season_cluster() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("01.mkv"), b"a").unwrap();
        std::fs::write(dir.path().join("02.mkv"), b"b").unwrap();
        std::fs::write(dir.path().join("03.mkv"), b"c").unwrap();

        let mut a = make_item(dir.path().join("01.mkv"), 41);
        let mut b = make_item(dir.path().join("02.mkv"), 42);
        a.hash.as_mut().unwrap().full_hash = Some(41);
        b.hash.as_mut().unwrap().full_hash = Some(42);
        a.media_type = MediaType::TvShow;
        b.media_type = MediaType::TvShow;
        a = with_identity_state(a, ConfirmationState::Confirmed);
        b = with_identity_state(b, ConfirmationState::Confirmed);
        a = with_tmdb_episode(a, "Dark Matter", 196147, 1, 1);
        b = with_tmdb_episode(b, "Dark Matter", 196147, 1, 1);
        a.parsed = Some(ParsedInfo {
            raw_title: "01".into(),
            year: Some(2024),
            season: Some(1),
            episode: Some(1),
            resolution: None,
            codec: None,
            source: None,
            release_group: None,
            media_suffix: None,
            parse_source: ParseSource::Regex,
            confidence: 0.4,
            evidence: vec![],
        });
        b.parsed = a.parsed.clone();

        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::HighestQuality,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].content_id, "tmdb:tv:196147:S01E01:season_cluster");
    }
}
