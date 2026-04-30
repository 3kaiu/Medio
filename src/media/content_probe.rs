use crate::models::media::{
    AudioEvidence, ContainerEvidence, ContentEvidence, MediaItem, SubtitleEvidence,
    SubtitleEvidenceSource, VisualEvidence,
};
use regex::Regex;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub struct ContentProbe;

impl ContentProbe {
    pub fn probe(item: &MediaItem) -> ContentEvidence {
        let mut evidence = ContentEvidence {
            runtime_secs: item.quality.as_ref().and_then(|q| q.duration_secs),
            visual: vec![VisualEvidence {
                source: "unimplemented".into(),
                text_hits: Vec::new(),
            }],
            audio: vec![AudioEvidence {
                source: "unimplemented".into(),
                transcript_hits: Vec::new(),
            }],
            ..Default::default()
        };

        if let Ok(container) = probe_container(&item.path) {
            if let Some(title) = container.title.clone() {
                push_unique(
                    &mut evidence.title_candidates,
                    normalize_title_candidate(&title),
                );
            }
            if let Some(comment) = container.comment.clone() {
                maybe_push_short_title_candidate(&mut evidence.title_candidates, &comment);
            }
            for chapter in &container.chapters {
                maybe_push_short_title_candidate(&mut evidence.title_candidates, chapter);
                capture_episode_hints(
                    chapter,
                    &mut evidence.season_hypotheses,
                    &mut evidence.episode_hypotheses,
                );
            }
            evidence.container = container;
        } else {
            evidence
                .risk_flags
                .push("container probe unavailable or failed".into());
        }

        evidence.subtitles = collect_subtitle_evidence(&item.path);
        for subtitle in &evidence.subtitles {
            for candidate in &subtitle.title_candidates {
                push_unique(
                    &mut evidence.title_candidates,
                    normalize_title_candidate(candidate),
                );
            }
            if let Some(season) = subtitle.season {
                push_unique_u32(&mut evidence.season_hypotheses, season);
            }
            if let Some(episode) = subtitle.episode {
                push_unique_u32(&mut evidence.episode_hypotheses, episode);
            }
        }

        evidence.title_candidates.retain(|title| !title.is_empty());
        evidence
    }
}

fn probe_container(path: &Path) -> Result<ContainerEvidence, Box<dyn std::error::Error>> {
    if !which::which("ffprobe").is_ok() {
        return Err("ffprobe unavailable".into());
    }

    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
            "-show_chapters",
        ])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(format!("ffprobe failed for {}", path.display()).into());
    }

    let parsed: FfprobeContent = serde_json::from_slice(&output.stdout)?;
    let mut evidence = ContainerEvidence {
        format_name: parsed.format.format_name,
        title: parsed
            .format
            .tags
            .as_ref()
            .and_then(|tags| tags.title.clone())
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty()),
        comment: parsed
            .format
            .tags
            .as_ref()
            .and_then(|tags| tags.comment.clone())
            .map(|comment| comment.trim().to_string())
            .filter(|comment| !comment.is_empty()),
        chapters: parsed
            .chapters
            .into_iter()
            .filter_map(|chapter| chapter.tags.and_then(|tags| tags.title))
            .map(|chapter| chapter.trim().to_string())
            .filter(|chapter| !chapter.is_empty())
            .collect(),
        ..Default::default()
    };

    for stream in parsed.streams {
        if let Some(tags) = stream.tags {
            if let Some(lang) = tags.language {
                push_unique(&mut evidence.stream_languages, lang);
            }
            if let Some(title) = tags.title {
                let title = title.trim().to_string();
                if !title.is_empty() {
                    push_unique(&mut evidence.track_titles, title);
                }
            }
        }
    }

    Ok(evidence)
}

fn collect_subtitle_evidence(media_path: &Path) -> Vec<SubtitleEvidence> {
    let mut evidence = Vec::new();
    for subtitle_path in find_external_subtitles(media_path) {
        if let Some(parsed) = parse_subtitle_file(&subtitle_path) {
            evidence.push(parsed);
        }
    }
    evidence.extend(collect_embedded_subtitle_evidence(media_path));
    evidence
}

fn collect_embedded_subtitle_evidence(media_path: &Path) -> Vec<SubtitleEvidence> {
    if !which::which("ffprobe").is_ok() || !which::which("ffmpeg").is_ok() {
        return Vec::new();
    }

    let Ok(output) = std::process::Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json", "-show_streams"])
        .arg(media_path)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let Ok(parsed) = serde_json::from_slice::<FfprobeSubtitleListing>(&output.stdout) else {
        return Vec::new();
    };

    let mut streams = parsed
        .streams
        .into_iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("subtitle"))
        .filter(|stream| is_text_subtitle_codec(stream.codec_name.as_deref()))
        .collect::<Vec<_>>();
    streams.sort_by(|a, b| subtitle_stream_priority(b).cmp(&subtitle_stream_priority(a)));
    streams
        .into_iter()
        .take(3)
        .filter_map(|stream| extract_embedded_subtitle_stream(media_path, stream))
        .collect()
}

fn find_external_subtitles(media_path: &Path) -> Vec<PathBuf> {
    let Some(dir) = media_path.parent() else {
        return Vec::new();
    };
    let Some(stem) = media_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
    else {
        return Vec::new();
    };

    let subtitle_exts = ["srt", "ass", "ssa", "vtt"];
    let mut matches = Vec::new();
    let mut fallback = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !subtitle_exts.contains(&ext.as_str()) {
                continue;
            }

            let sub_stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if sub_stem == stem
                || sub_stem.starts_with(&format!("{stem}."))
                || sub_stem.starts_with(&format!("{stem}-"))
            {
                matches.push(path);
            } else {
                fallback.push(path);
            }
        }
    }

    if !matches.is_empty() {
        matches
    } else if fallback.len() == 1 {
        fallback
    } else {
        Vec::new()
    }
}

fn parse_subtitle_file(path: &Path) -> Option<SubtitleEvidence> {
    let content = std::fs::read_to_string(path).ok()?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    parse_subtitle_content(
        &content,
        match ext.as_str() {
            "ass" | "ssa" => SubtitleTextFormat::Ass,
            _ => SubtitleTextFormat::Plain,
        },
        SubtitleEvidenceSource::ExternalText,
        path.display().to_string(),
        None,
        None,
    )
}

fn extract_embedded_subtitle_stream(
    media_path: &Path,
    stream: FfprobeSubtitleStream,
) -> Option<SubtitleEvidence> {
    let index = stream.index?;
    let output = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(media_path)
        .args(["-map", &format!("0:{index}"), "-f", "srt", "-"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let content = String::from_utf8(output.stdout).ok()?;
    let format = match stream.codec_name.as_deref() {
        Some("ass") | Some("ssa") => SubtitleTextFormat::Ass,
        _ => SubtitleTextFormat::Plain,
    };
    parse_subtitle_content(
        &content,
        format,
        SubtitleEvidenceSource::EmbeddedTrack,
        format!("embedded:stream:{index}"),
        stream.tags.as_ref().and_then(|tags| tags.language.clone()),
        stream.tags.as_ref().and_then(|tags| tags.title.clone()),
    )
}

fn parse_subtitle_content(
    content: &str,
    format: SubtitleTextFormat,
    source: SubtitleEvidenceSource,
    locator: String,
    language: Option<String>,
    track_title: Option<String>,
) -> Option<SubtitleEvidence> {
    let sample_lines = prioritized_subtitle_lines(content, format);
    if sample_lines.is_empty() {
        return None;
    }

    let mut title_candidates = Vec::new();
    let mut season = None;
    let mut episode = None;
    if let Some(track_title) = track_title.as_deref() {
        for candidate in
            extract_title_candidates_from_subtitle_line(track_title, language.as_deref())
        {
            maybe_push_short_title_candidate(&mut title_candidates, &candidate);
        }
        if let Some((s, e)) = detect_episode_marker(track_title) {
            season = season.or(s);
            episode = episode.or(e);
        }
    }
    for line in &sample_lines {
        for candidate in extract_title_candidates_from_subtitle_line(line, language.as_deref()) {
            maybe_push_short_title_candidate(&mut title_candidates, &candidate);
        }
        if let Some((s, e)) = detect_episode_marker(line) {
            season = season.or(s);
            episode = episode.or(e);
        }
    }
    title_candidates = normalize_title_aliases(title_candidates);

    Some(SubtitleEvidence {
        source,
        locator,
        language,
        track_title,
        sample_lines,
        title_candidates,
        season,
        episode,
    })
}

fn prioritized_subtitle_lines(content: &str, format: SubtitleTextFormat) -> Vec<String> {
    let raw_lines = match format {
        SubtitleTextFormat::Ass => extract_ass_lines(content),
        SubtitleTextFormat::Plain => extract_text_lines(content),
    };
    if raw_lines.is_empty() {
        return raw_lines;
    }

    let mut prioritized = raw_lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            (
                subtitle_line_priority(line),
                std::cmp::Reverse(index),
                line.clone(),
            )
        })
        .collect::<Vec<_>>();
    prioritized.sort_by(|a, b| b.cmp(a));

    let mut selected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (_, _, line) in prioritized {
        let key = normalize_title_candidate(&line).to_ascii_lowercase();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        selected.push(line);
        if selected.len() >= 12 {
            break;
        }
    }
    selected
}

fn extract_text_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in content.lines() {
        let line = clean_subtitle_line(raw);
        if line.is_empty() || is_subtitle_noise(&line) {
            continue;
        }
        lines.push(line);
        if lines.len() >= 64 {
            break;
        }
    }
    lines
}

fn extract_ass_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in content.lines() {
        let raw = raw.trim();
        if !raw.starts_with("Dialogue:") {
            continue;
        }
        let payload = raw.splitn(10, ',').nth(9).unwrap_or_default();
        let line = clean_subtitle_line(payload);
        if line.is_empty() || is_subtitle_noise(&line) {
            continue;
        }
        lines.push(line);
        if lines.len() >= 64 {
            break;
        }
    }
    lines
}

fn clean_subtitle_line(input: &str) -> String {
    let tag_re = Regex::new(r"\{[^}]+\}|<[^>]+>").unwrap();
    let ts_re = Regex::new(r"^\d{2}:\d{2}:\d{2}").unwrap();
    let seq_re = Regex::new(r"^\d+$").unwrap();

    let line = input.trim().replace("\\N", " ");
    if ts_re.is_match(&line) || seq_re.is_match(&line) {
        return String::new();
    }

    tag_re
        .replace_all(&line, "")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

fn extract_title_candidates_from_subtitle_line(line: &str, language: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    let patterns = [
        r"(?i)^previously on\s+(.+)$",
        r"(?i)^last time on\s+(.+)$",
        r"(?i)^you(?:'re| are) watching\s+(.+)$",
        r"(?i)^watching\s+(.+)$",
        r"(?i)^from the series\s+(.+)$",
        r"(?i)^(.+?)\s*-\s*S\d{1,2}E\d{1,3}\b",
        r"(?i)^(.+?)\s+season\s+\d{1,2}\s+episode\s+\d{1,3}\b",
        r"(?i)^(.+?)\s+ep(?:isode)?\s+\d{1,3}\b",
        r#"^[\"'“《](.+?)[\"'”》]\s*(?:第\s*\d+\s*集|episode\s*\d+|ep\s*\d+|s\d{1,2}e\d{1,3})"#,
        r#"^(.+?)\s*[·:：]\s*(?:第\s*\d+\s*集|episode\s*\d+|ep\s*\d+|s\d{1,2}e\d{1,3})"#,
    ];

    for pattern in patterns {
        let regex = Regex::new(pattern).unwrap();
        if let Some(caps) = regex.captures(line)
            && let Some(matched) = caps.get(1)
        {
            let title = matched.as_str().trim();
            if !title.is_empty() {
                candidates.push(title.to_string());
            }
        }
    }

    if let Some((_, episode_hint)) = detect_episode_marker(line)
        && let Some(prefix) = line
            .split(['-', '|', ':'])
            .next()
            .map(str::trim)
            .filter(|prefix| prefix.len() > 3 && prefix.len() <= 48)
        && !prefix.to_ascii_lowercase().contains("episode")
        && episode_hint
            .map(|episode| !prefix.contains(&episode.to_string()))
            .unwrap_or(true)
    {
        candidates.push(prefix.to_string());
    }

    let cn_recap_patterns = [
        r"^(.+?)\s*前情提要$",
        r"^(.+?)\s*上[一二三四五六七八九0-9]*集回顾$",
        r"^(.+?)\s*第\s*\d+\s*集$",
        r"^第\s*\d+\s*集\s*(.+)$",
    ];
    for pattern in cn_recap_patterns {
        let regex = Regex::new(pattern).unwrap();
        if let Some(caps) = regex.captures(line)
            && let Some(matched) = caps.get(1)
        {
            let title = matched.as_str().trim();
            if !title.is_empty() {
                candidates.push(title.to_string());
            }
        }
    }

    candidates.extend(split_multilingual_title_candidates(line, language));

    candidates
}

fn is_subtitle_noise(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.len() < 2 || trimmed.starts_with('[') && trimmed.ends_with(']')
}

fn maybe_push_short_title_candidate(candidates: &mut Vec<String>, raw: &str) {
    let cleaned = normalize_title_candidate(raw);
    if cleaned.is_empty() {
        return;
    }
    let word_count = cleaned.split_whitespace().count();
    if word_count <= 10 && cleaned.len() <= 64 {
        push_unique(candidates, cleaned);
    }
}

fn normalize_title_aliases(candidates: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for candidate in candidates {
        let cleaned = normalize_title_candidate(&candidate);
        if cleaned.is_empty() || !looks_like_title_candidate(&cleaned) {
            continue;
        }

        let canonical = canonical_title_alias_key(&cleaned);
        if seen.insert(canonical) {
            normalized.push(cleaned);
        }
    }

    normalized
}

fn normalize_title_candidate(raw: &str) -> String {
    let punct_re = Regex::new(r"[._]+").unwrap();
    let ws_re = Regex::new(r"\s+").unwrap();
    ws_re
        .replace_all(
            &punct_re.replace_all(
                raw.trim()
                    .trim_matches(|c: char| matches!(c, '-' | ':' | '|' | '"' | '\'')),
                " ",
            ),
            " ",
        )
        .trim()
        .to_string()
}

fn canonical_title_alias_key(raw: &str) -> String {
    let alias_re = Regex::new(r#"[·:：\-_'\"“”‘’《》()\[\]]"#).unwrap();
    alias_re
        .replace_all(&normalize_title_candidate(raw).to_ascii_lowercase(), " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn detect_episode_marker(raw: &str) -> Option<(Option<u32>, Option<u32>)> {
    let se_re = Regex::new(r"(?i)S(\d{1,2})E(\d{1,3})").unwrap();
    let ep_re = Regex::new(r"(?i)(?:Episode|EP|E)\s*(\d{1,3})").unwrap();
    let cn_ep_re = Regex::new(r"第\s*(\d{1,3})\s*集").unwrap();

    if let Some(caps) = se_re.captures(raw) {
        let season = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let episode = caps.get(2).and_then(|m| m.as_str().parse().ok());
        return Some((season, episode));
    }
    if let Some(caps) = ep_re.captures(raw) {
        let episode = caps.get(1).and_then(|m| m.as_str().parse().ok());
        return Some((None, episode));
    }
    if let Some(caps) = cn_ep_re.captures(raw) {
        let episode = caps.get(1).and_then(|m| m.as_str().parse().ok());
        return Some((None, episode));
    }
    None
}

fn capture_episode_hints(raw: &str, seasons: &mut Vec<u32>, episodes: &mut Vec<u32>) {
    if let Some((season, episode)) = detect_episode_marker(raw) {
        if let Some(season) = season {
            push_unique_u32(seasons, season);
        }
        if let Some(episode) = episode {
            push_unique_u32(episodes, episode);
        }
    }
}

fn push_unique(values: &mut Vec<String>, candidate: String) {
    let candidate = candidate.trim().to_string();
    if candidate.is_empty() {
        return;
    }
    if !values.iter().any(|existing| existing == &candidate) {
        values.push(candidate);
    }
}

fn push_unique_u32(values: &mut Vec<u32>, candidate: u32) {
    if !values.contains(&candidate) {
        values.push(candidate);
    }
}

fn subtitle_line_priority(line: &str) -> (u8, u8, usize) {
    let lower = line.to_ascii_lowercase();
    let signal_score = if lower.contains("previously on")
        || lower.contains("last time on")
        || line.contains("前情提要")
        || line.contains("上集回顾")
    {
        4
    } else if lower.contains("you are watching")
        || lower.contains("you're watching")
        || lower.contains("from the series")
    {
        3
    } else if detect_episode_marker(line).is_some() {
        2
    } else {
        1
    };
    let length_score = if (4..=48).contains(&line.chars().count()) {
        2
    } else {
        1
    };
    (signal_score, length_score, usize::MAX - line.len())
}

fn split_multilingual_title_candidates(line: &str, language: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    let separators = [" / ", " | ", " - ", " – ", " — ", "·", "：", ":"];

    for separator in separators {
        if let Some((left, right)) = line.split_once(separator) {
            let left = strip_title_prompt_prefix(&normalize_title_candidate(left));
            let right = strip_title_prompt_prefix(&normalize_title_candidate(right));
            if looks_like_title_candidate(&left)
                && looks_like_title_candidate(&right)
                && (title_scripts_differ(&left, &right) || language_suggests_bilingual(language))
            {
                candidates.push(left);
                candidates.push(right);
            }
        }
    }

    candidates
}

fn strip_title_prompt_prefix(candidate: &str) -> String {
    let prefixes = [
        "请继续观看 ",
        "继续观看 ",
        "正在播放 ",
        "请收看 ",
        "you are watching ",
        "you're watching ",
        "watching ",
        "from the series ",
    ];
    let trimmed = candidate.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            return trimmed[prefix.len()..].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn looks_like_title_candidate(candidate: &str) -> bool {
    let count = candidate.chars().count();
    if !(2..=64).contains(&count) {
        return false;
    }
    let lower = candidate.to_ascii_lowercase();
    if lower.starts_with("episode ")
        || lower.starts_with("season ")
        || lower.contains("subtitle")
        || lower.contains("dialogue")
        || lower.contains("commentary")
    {
        return false;
    }
    candidate.chars().any(|ch| ch.is_alphabetic() || is_cjk(ch))
}

fn language_suggests_bilingual(language: Option<&str>) -> bool {
    matches!(
        language.unwrap_or_default().to_ascii_lowercase().as_str(),
        "chi" | "zho" | "zh" | "chs" | "cht" | "jpn" | "ja" | "kor" | "ko"
    )
}

fn title_scripts_differ(left: &str, right: &str) -> bool {
    let left_ascii = left.is_ascii();
    let right_ascii = right.is_ascii();
    if left_ascii != right_ascii {
        return true;
    }
    left.chars().any(is_cjk) != right.chars().any(is_cjk)
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
    )
}

fn is_text_subtitle_codec(codec_name: Option<&str>) -> bool {
    matches!(
        codec_name.unwrap_or_default(),
        "subrip" | "srt" | "ass" | "ssa" | "webvtt" | "mov_text" | "text"
    )
}

fn subtitle_stream_priority(stream: &FfprobeSubtitleStream) -> (u8, u8, u8, u8) {
    let language_score = stream
        .tags
        .as_ref()
        .and_then(|tags| tags.language.as_deref())
        .map(score_subtitle_language)
        .unwrap_or(0);
    let title_score = stream
        .tags
        .as_ref()
        .and_then(|tags| tags.title.as_deref())
        .map(score_subtitle_track_title)
        .unwrap_or(0);
    let default_score = stream
        .disposition
        .as_ref()
        .map(|disposition| disposition.default.unwrap_or(0) as u8)
        .unwrap_or(0);
    let forced_penalty = stream
        .disposition
        .as_ref()
        .map(|disposition| disposition.forced.unwrap_or(0) as u8)
        .unwrap_or(0);
    (
        language_score,
        title_score,
        default_score,
        1u8.saturating_sub(forced_penalty),
    )
}

fn score_subtitle_language(language: &str) -> u8 {
    match language.trim().to_ascii_lowercase().as_str() {
        "eng" | "en" | "chi" | "zho" | "zh" | "chs" | "cht" | "jpn" | "ja" | "kor" | "ko" => 3,
        "" | "und" => 1,
        _ => 2,
    }
}

fn score_subtitle_track_title(track_title: &str) -> u8 {
    let lower = track_title.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return 0;
    }
    let mut score: u8 = 1;
    if lower.contains("full") || lower.contains("dialog") || lower.contains("english") {
        score += 1;
    }
    if lower.contains("forced") || lower.contains("sdh") || lower.contains("commentary") {
        score = score.saturating_sub(1);
    }
    score
}

#[derive(Debug, Clone, Copy)]
enum SubtitleTextFormat {
    Plain,
    Ass,
}

#[derive(Debug, Deserialize)]
struct FfprobeContent {
    #[serde(default)]
    streams: Vec<FfprobeContentStream>,
    format: FfprobeContentFormat,
    #[serde(default)]
    chapters: Vec<FfprobeContentChapter>,
}

#[derive(Debug, Deserialize)]
struct FfprobeContentStream {
    tags: Option<FfprobeContentTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeContentFormat {
    format_name: Option<String>,
    tags: Option<FfprobeContentTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeContentChapter {
    tags: Option<FfprobeContentTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeContentTags {
    title: Option<String>,
    comment: Option<String>,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSubtitleListing {
    #[serde(default)]
    streams: Vec<FfprobeSubtitleStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSubtitleStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    tags: Option<FfprobeContentTags>,
    disposition: Option<FfprobeStreamDisposition>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStreamDisposition {
    default: Option<u32>,
    forced: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_episode_marker_in_subtitles() {
        let marker = detect_episode_marker("Season 1 Episode 3").unwrap();
        assert_eq!(marker.1, Some(3));
    }

    #[test]
    fn parses_ass_dialogue_lines() {
        let content =
            "Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Previously on Breaking Bad";
        let lines = extract_ass_lines(content);
        assert_eq!(lines[0], "Previously on Breaking Bad");
    }

    #[test]
    fn extracts_show_title_candidates_from_subtitle_lines() {
        let evidence = parse_subtitle_content(
            "1\n00:00:01,000 --> 00:00:03,000\nPreviously on Breaking Bad\n",
            SubtitleTextFormat::Plain,
            SubtitleEvidenceSource::EmbeddedTrack,
            "embedded:stream:3".into(),
            Some("eng".into()),
            Some("English".into()),
        )
        .unwrap();
        assert_eq!(evidence.title_candidates, vec!["Breaking Bad"]);
        assert_eq!(evidence.source, SubtitleEvidenceSource::EmbeddedTrack);
        assert_eq!(evidence.language.as_deref(), Some("eng"));
    }

    #[test]
    fn prioritizes_recap_lines_over_plain_dialogue() {
        let mut content = String::new();
        for _ in 0..20 {
            content.push_str("Hello there\n");
        }
        content.push_str("Previously on Severance\n");

        let evidence = parse_subtitle_content(
            &content,
            SubtitleTextFormat::Plain,
            SubtitleEvidenceSource::ExternalText,
            "/tmp/demo.srt".into(),
            Some("eng".into()),
            None,
        )
        .unwrap();

        assert!(
            evidence
                .sample_lines
                .iter()
                .any(|line| line == "Previously on Severance")
        );
        assert!(
            evidence
                .title_candidates
                .iter()
                .any(|title| title == "Severance")
        );
    }

    #[test]
    fn recognizes_text_subtitle_codecs() {
        assert!(is_text_subtitle_codec(Some("subrip")));
        assert!(is_text_subtitle_codec(Some("mov_text")));
        assert!(!is_text_subtitle_codec(Some("hdmv_pgs_subtitle")));
    }

    #[test]
    fn extracts_chinese_recap_title_candidates() {
        let evidence = parse_subtitle_content(
            "某某神剧 前情提要\n第12集 某某神剧\n",
            SubtitleTextFormat::Plain,
            SubtitleEvidenceSource::ExternalText,
            "/tmp/demo.srt".into(),
            Some("chi".into()),
            None,
        )
        .unwrap();
        assert!(
            evidence
                .title_candidates
                .iter()
                .any(|title| title == "某某神剧")
        );
        assert_eq!(evidence.episode, Some(12));
    }

    #[test]
    fn splits_bilingual_title_aliases() {
        let evidence = parse_subtitle_content(
            "请继续观看 狂飙 / The Knockout\n",
            SubtitleTextFormat::Plain,
            SubtitleEvidenceSource::ExternalText,
            "/tmp/demo.srt".into(),
            Some("chi".into()),
            None,
        )
        .unwrap();

        assert!(
            evidence
                .title_candidates
                .iter()
                .any(|title| title == "狂飙")
        );
        assert!(
            evidence
                .title_candidates
                .iter()
                .any(|title| title == "The Knockout")
        );
    }

    #[test]
    fn prioritizes_default_full_dialog_subtitle_streams() {
        let high = FfprobeSubtitleStream {
            index: Some(2),
            codec_type: Some("subtitle".into()),
            codec_name: Some("subrip".into()),
            tags: Some(FfprobeContentTags {
                title: Some("English Full".into()),
                comment: None,
                language: Some("eng".into()),
            }),
            disposition: Some(FfprobeStreamDisposition {
                default: Some(1),
                forced: Some(0),
            }),
        };
        let low = FfprobeSubtitleStream {
            index: Some(3),
            codec_type: Some("subtitle".into()),
            codec_name: Some("subrip".into()),
            tags: Some(FfprobeContentTags {
                title: Some("English Forced".into()),
                comment: None,
                language: Some("eng".into()),
            }),
            disposition: Some(FfprobeStreamDisposition {
                default: Some(0),
                forced: Some(1),
            }),
        };

        assert!(subtitle_stream_priority(&high) > subtitle_stream_priority(&low));
    }
}
