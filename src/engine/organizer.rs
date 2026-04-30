use crate::core::config::OrganizeConfig;
use crate::core::types::{LinkMode, OrganizeMode};
use crate::engine::execution_report::ExecutionReport;
use crate::engine::nfo_writer;
use crate::models::media::{MediaItem, MediaType, MetadataOrigin, ScrapeSource};
use crate::scraper::image_scraper;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct OrganizePlan {
    pub source: PathBuf,
    pub target: PathBuf,
    pub action: OrganizeAction,
    pub nfo_content: Option<String>,
    pub image_urls: Vec<String>,
    pub decision: OrganizeDecisionProfile,
    pub rationale: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrganizeAction {
    Move,
    #[allow(dead_code)]
    Copy,
    HardLink,
    SymLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum AssetGateStatus {
    Accepted,
    DisabledByConfig,
    #[default]
    MissingMetadata,
    IdentityUnconfirmed,
    UntrustedSource,
    LowAuthority,
    MissingAssetUrls,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AssetGateDecision {
    pub status: AssetGateStatus,
    pub threshold: Option<f32>,
    pub authority: Option<f32>,
    pub provider: Option<ScrapeSource>,
    pub identity_confirmation: Option<String>,
    pub asset_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrganizeDecisionProfile {
    pub mode: OrganizeMode,
    pub action: OrganizeAction,
    pub title_origin: Option<MetadataOrigin>,
    pub title_confidence: Option<f32>,
    pub year_origin: Option<MetadataOrigin>,
    pub year_confidence: Option<f32>,
    pub season_origin: Option<MetadataOrigin>,
    pub season_confidence: Option<f32>,
    pub nfo_gate: AssetGateDecision,
    pub image_gate: AssetGateDecision,
}

impl Default for OrganizeDecisionProfile {
    fn default() -> Self {
        Self {
            mode: OrganizeMode::Archive,
            action: OrganizeAction::Move,
            title_origin: None,
            title_confidence: None,
            year_origin: None,
            year_confidence: None,
            season_origin: None,
            season_confidence: None,
            nfo_gate: AssetGateDecision::default(),
            image_gate: AssetGateDecision::default(),
        }
    }
}

pub struct Organizer {
    config: OrganizeConfig,
}

impl Organizer {
    pub fn new(config: OrganizeConfig) -> Self {
        Self { config }
    }

    /// Generate organize plans for all items
    pub fn plan(
        &self,
        items: &[MediaItem],
        mode: OrganizeMode,
        link: LinkMode,
    ) -> Vec<OrganizePlan> {
        let root = if self.config.root.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            self.config.root.clone()
        };

        let mut plans: Vec<OrganizePlan> = items
            .iter()
            .filter_map(|item| {
                let target_dir = self.target_dir(item, &root, &mode)?;
                let filename = item.path.file_name()?.to_os_string();
                let target = target_dir.join(&filename);

                // Skip if already in the right place
                if item.path.parent() == Some(&target_dir) {
                    return None;
                }

                let action = match link {
                    LinkMode::None => match mode {
                        OrganizeMode::Archive | OrganizeMode::Local => OrganizeAction::Move,
                        OrganizeMode::Rename => OrganizeAction::Move,
                    },
                    LinkMode::Hard => OrganizeAction::HardLink,
                    LinkMode::Sym => OrganizeAction::SymLink,
                };

                let mut rationale = organize_rationale(item, mode, action, &root, &target_dir);

                let (nfo_content, nfo_gate) = if self.config.with_nfo {
                    build_nfo_content(item)
                } else {
                    (
                        None,
                        AssetGateDecision {
                            status: AssetGateStatus::DisabledByConfig,
                            ..Default::default()
                        },
                    )
                };
                rationale.push(describe_asset_gate("nfo", &nfo_gate));

                let (image_urls, image_gate) = if self.config.with_images {
                    build_image_urls(item)
                } else {
                    (
                        Vec::new(),
                        AssetGateDecision {
                            status: AssetGateStatus::DisabledByConfig,
                            ..Default::default()
                        },
                    )
                };
                rationale.push(describe_asset_gate("image", &image_gate));

                Some(OrganizePlan {
                    source: item.path.clone(),
                    target,
                    action,
                    nfo_content,
                    image_urls,
                    decision: organize_decision(item, mode, action, nfo_gate, image_gate),
                    rationale,
                    conflicts: Vec::new(),
                })
            })
            .collect();

        self.apply_preflight_conflicts(&mut plans);
        plans
    }

    fn target_dir(&self, item: &MediaItem, root: &Path, mode: &OrganizeMode) -> Option<PathBuf> {
        match mode {
            OrganizeMode::Rename => {
                // Just rename in place, no directory reorganization
                item.path.parent().map(|p| p.to_path_buf())
            }
            OrganizeMode::Archive => {
                // Organize into: root/MediaType/Title/Season/
                let type_dir = match item.media_type {
                    MediaType::Movie => "Movies",
                    MediaType::TvShow => "TV Shows",
                    MediaType::Music => "Music",
                    MediaType::Novel => "Books",
                    MediaType::Strm => "TV Shows",
                    MediaType::Unknown => "Other",
                };

                let title = item
                    .preferred_title()
                    .map(|decision| sanitize_filename(&decision.value))
                    .unwrap_or_else(|| "Unknown".into());

                let mut dir = root.join(type_dir).join(&title);

                // Add season subdirectory for TV
                if item.media_type == MediaType::TvShow
                    && let Some(s) = item.preferred_season().map(|decision| decision.value)
                {
                    dir = dir.join(format!("Season {s:02}"));
                }

                // Add artist/album for music
                if item.media_type == MediaType::Music
                    && let Some(artist) = item.scraped.as_ref().and_then(|s| s.artist.as_ref())
                {
                    dir = root
                        .join(type_dir)
                        .join(sanitize_filename(artist))
                        .join(&title);
                }

                Some(dir)
            }
            OrganizeMode::Local => {
                // Organize within the same parent directory
                let parent = item.path.parent()?;
                let title = item
                    .preferred_title()
                    .map(|decision| sanitize_filename(&decision.value));

                if let Some(title) = title {
                    let mut dir = parent.join(&title);
                    if item.media_type == MediaType::TvShow
                        && let Some(s) = item.preferred_season().map(|decision| decision.value)
                    {
                        dir = dir.join(format!("Season {s:02}"));
                    }
                    Some(dir)
                } else {
                    None
                }
            }
        }
    }

    /// Execute organize plans (dry-run supported)
    #[allow(dead_code)]
    pub fn execute(&self, plans: &[OrganizePlan], dry_run: bool) -> Vec<String> {
        self.execute_report(plans, dry_run).details
    }

    pub fn execute_report(&self, plans: &[OrganizePlan], dry_run: bool) -> ExecutionReport {
        let mut report = ExecutionReport::new("organize");

        // Reuse HTTP client for all image downloads
        let img_client = if plans.iter().any(|p| !p.image_urls.is_empty()) {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .ok()
        } else {
            None
        };

        for plan in plans {
            if !plan.conflicts.is_empty() {
                report.blocked += 1;
                report
                    .details
                    .push(format!("[conflict] {}", plan.source.display()));
                for conflict in &plan.conflicts {
                    report.details.push(format!("  - {conflict}"));
                }
                continue;
            }
            // Create target directory
            if let Some(parent) = plan.target.parent() {
                if dry_run {
                    report
                        .details
                        .push(format!("[dry-run] mkdir -p {}", parent.display()));
                } else if !parent.exists()
                    && let Err(e) = std::fs::create_dir_all(parent)
                {
                    report.errors += 1;
                    report
                        .details
                        .push(format!("[error] mkdir {}: {e}", parent.display()));
                    continue;
                }
            }

            // Move/copy/link file
            let action_label = match plan.action {
                OrganizeAction::Move => "move",
                OrganizeAction::Copy => "copy",
                OrganizeAction::HardLink => "hardlink",
                OrganizeAction::SymLink => "symlink",
            };

            if dry_run {
                report.executed += 1;
                report.details.push(format!(
                    "[dry-run] {action_label} {} → {}",
                    plan.source.display(),
                    plan.target.display()
                ));
            } else {
                let result = match plan.action {
                    OrganizeAction::Move => std::fs::rename(&plan.source, &plan.target),
                    OrganizeAction::Copy => std::fs::copy(&plan.source, &plan.target).map(|_| ()),
                    OrganizeAction::HardLink => std::fs::hard_link(&plan.source, &plan.target),
                    OrganizeAction::SymLink => {
                        if plan.target.exists() {
                            Ok(())
                        } else {
                            std::os::unix::fs::symlink(&plan.source, &plan.target)
                        }
                    }
                };
                match result {
                    Ok(()) => {
                        let msg = format!(
                            "[{action_label}] {} → {}",
                            plan.source.display(),
                            plan.target.display()
                        );
                        crate::core::oplog::log(&msg);
                        report.executed += 1;
                        report.details.push(msg);
                    }
                    Err(e) => {
                        report.errors += 1;
                        report.details.push(format!(
                            "[error] {} → {}: {e}",
                            plan.source.display(),
                            plan.target.display()
                        ));
                    }
                }
            }

            // Write NFO
            if let Some(nfo) = &plan.nfo_content {
                let nfo_path = plan.target.with_extension("nfo");
                if dry_run {
                    report.asset_generated += 1;
                    report
                        .details
                        .push(format!("[dry-run] write nfo {}", nfo_path.display()));
                } else if let Err(e) = std::fs::write(&nfo_path, nfo) {
                    report.errors += 1;
                    report
                        .details
                        .push(format!("[error] write nfo {}: {e}", nfo_path.display()));
                } else {
                    report.asset_generated += 1;
                    report.details.push(format!("[nfo] {}", nfo_path.display()));
                }
            }

            // Download images
            for (idx, url) in plan.image_urls.iter().enumerate() {
                let img_dir = plan.target.parent().unwrap_or(Path::new("."));
                let img_path = image_scraper::build_image_path(img_dir, idx, url);

                if dry_run {
                    report.asset_generated += 1;
                    report
                        .details
                        .push(format!("[dry-run] download image from {url}"));
                } else if let Some(ref client) = img_client {
                    match image_scraper::download(client, url, &img_path) {
                        Ok(()) => {
                            report.asset_generated += 1;
                            report
                                .details
                                .push(format!("[image] {}", img_path.display()));
                        }
                        Err(e) => {
                            report.errors += 1;
                            report.details.push(format!("[error] {e}"));
                        }
                    }
                }
            }
        }

        // Cleanup empty directories
        if self.config.cleanup_empty_dirs && !dry_run {
            let mut dirs: Vec<PathBuf> = plans
                .iter()
                .filter_map(|p| p.source.parent().map(|d| d.to_path_buf()))
                .collect();
            dirs.sort();
            dirs.dedup();
            for dir in dirs.iter().rev() {
                if dir.exists()
                    && std::fs::read_dir(dir)
                        .map(|mut d| d.next().is_none())
                        .unwrap_or(false)
                    && let Ok(()) = std::fs::remove_dir(dir)
                {
                    report.executed += 1;
                    report
                        .details
                        .push(format!("[cleanup] removed empty dir {}", dir.display()));
                }
            }
        }

        report
    }

    fn apply_preflight_conflicts(&self, plans: &mut [OrganizePlan]) {
        let mut target_counts: std::collections::HashMap<PathBuf, usize> =
            std::collections::HashMap::new();
        for plan in plans.iter() {
            *target_counts.entry(plan.target.clone()).or_default() += 1;
        }

        for plan in plans.iter_mut() {
            if target_counts.get(&plan.target).copied().unwrap_or(0) > 1 {
                plan.conflicts.push(format!(
                    "target path collides with another organize plan: {}",
                    plan.target.display()
                ));
            }
            if plan.target.exists() && plan.target != plan.source {
                plan.conflicts.push(format!(
                    "target path already exists on disk: {}",
                    plan.target.display()
                ));
            }
            if plan.nfo_content.is_some() {
                let nfo_path = plan.target.with_extension("nfo");
                if nfo_path.exists() {
                    plan.conflicts.push(format!(
                        "nfo target already exists on disk: {}",
                        nfo_path.display()
                    ));
                }
            }
            for (idx, url) in plan.image_urls.iter().enumerate() {
                let img_dir = plan.target.parent().unwrap_or(Path::new("."));
                let img_path = image_scraper::build_image_path(img_dir, idx, url);
                if img_path.exists() {
                    plan.conflicts.push(format!(
                        "image target already exists on disk: {}",
                        img_path.display()
                    ));
                }
            }
        }
    }
}

fn build_nfo_content(item: &MediaItem) -> (Option<String>, AssetGateDecision) {
    let Some(scraped) = item.scraped.as_ref() else {
        return (None, AssetGateDecision::default());
    };
    let authority = item.scraped_metadata_confidence();
    let identity_confirmation = item.identity_confirmation_label().map(ToString::to_string);
    if scraped.source == ScrapeSource::Guess {
        return (
            None,
            AssetGateDecision {
                status: AssetGateStatus::UntrustedSource,
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                ..Default::default()
            },
        );
    }
    let Some(threshold) = item.canonical_nfo_authority_threshold() else {
        return (
            None,
            AssetGateDecision {
                status: AssetGateStatus::IdentityUnconfirmed,
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                ..Default::default()
            },
        );
    };
    if authority < threshold {
        return (
            None,
            AssetGateDecision {
                status: AssetGateStatus::LowAuthority,
                threshold: Some(threshold),
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                ..Default::default()
            },
        );
    }
    let content = nfo_writer::generate(item);
    let asset_count = usize::from(content.is_some());
    (
        content,
        AssetGateDecision {
            status: AssetGateStatus::Accepted,
            threshold: Some(threshold),
            authority: Some(authority),
            provider: Some(scraped.source),
            identity_confirmation,
            asset_count,
        },
    )
}

fn build_image_urls(item: &MediaItem) -> (Vec<String>, AssetGateDecision) {
    let Some(scraped) = item.scraped.as_ref() else {
        return (Vec::new(), AssetGateDecision::default());
    };
    let authority = item.scraped_metadata_confidence();
    let identity_confirmation = item.identity_confirmation_label().map(ToString::to_string);
    if matches!(scraped.source, ScrapeSource::Guess | ScrapeSource::AiAssist) {
        return (
            Vec::new(),
            AssetGateDecision {
                status: AssetGateStatus::UntrustedSource,
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                ..Default::default()
            },
        );
    }
    let Some(threshold) = item.canonical_asset_authority_threshold() else {
        return (
            Vec::new(),
            AssetGateDecision {
                status: AssetGateStatus::IdentityUnconfirmed,
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                ..Default::default()
            },
        );
    };
    if authority < threshold {
        return (
            Vec::new(),
            AssetGateDecision {
                status: AssetGateStatus::LowAuthority,
                threshold: Some(threshold),
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                ..Default::default()
            },
        );
    }
    let urls = image_scraper::collect_urls(scraped);
    if urls.is_empty() {
        (
            Vec::new(),
            AssetGateDecision {
                status: AssetGateStatus::MissingAssetUrls,
                authority: Some(authority),
                provider: Some(scraped.source),
                threshold: Some(threshold),
                identity_confirmation,
                ..Default::default()
            },
        )
    } else {
        let count = urls.len();
        (
            urls,
            AssetGateDecision {
                status: AssetGateStatus::Accepted,
                threshold: Some(threshold),
                authority: Some(authority),
                provider: Some(scraped.source),
                identity_confirmation,
                asset_count: count,
            },
        )
    }
}

fn organize_decision(
    item: &MediaItem,
    mode: OrganizeMode,
    action: OrganizeAction,
    nfo_gate: AssetGateDecision,
    image_gate: AssetGateDecision,
) -> OrganizeDecisionProfile {
    let title = item.preferred_title();
    let year = item.preferred_year();
    let season = item.preferred_season();
    OrganizeDecisionProfile {
        mode,
        action,
        title_origin: title.as_ref().map(|decision| decision.origin),
        title_confidence: title.as_ref().map(|decision| decision.confidence),
        year_origin: year.as_ref().map(|decision| decision.origin),
        year_confidence: year.as_ref().map(|decision| decision.confidence),
        season_origin: season.as_ref().map(|decision| decision.origin),
        season_confidence: season.as_ref().map(|decision| decision.confidence),
        nfo_gate,
        image_gate,
    }
}

fn describe_asset_gate(kind: &str, gate: &AssetGateDecision) -> String {
    let identity_suffix = gate
        .identity_confirmation
        .as_deref()
        .map(|label| format!(" ({label} identity)"))
        .unwrap_or_default();
    match gate.status {
        AssetGateStatus::Accepted => match kind {
            "nfo" => format!("nfo gate: accepted trusted metadata{identity_suffix}"),
            _ => format!(
                "image gate: accepted {} trusted asset urls{}",
                gate.asset_count, identity_suffix
            ),
        },
        AssetGateStatus::DisabledByConfig => match kind {
            "nfo" => "nfo gate: generation disabled by config".into(),
            _ => "image gate: download disabled by config".into(),
        },
        AssetGateStatus::MissingMetadata => format!("{kind} gate: missing scraped metadata"),
        AssetGateStatus::IdentityUnconfirmed => format!(
            "{kind} gate: identity confirmation is not strong enough{}",
            identity_suffix
        ),
        AssetGateStatus::UntrustedSource => format!(
            "{kind} gate: {:?} metadata is not trusted{}",
            gate.provider.unwrap_or(ScrapeSource::Guess),
            identity_suffix
        ),
        AssetGateStatus::LowAuthority => format!(
            "{kind} gate: scraped authority {:.2} below threshold {:.2}{}",
            gate.authority.unwrap_or(0.0),
            gate.threshold.unwrap_or(0.0),
            identity_suffix
        ),
        AssetGateStatus::MissingAssetUrls => {
            format!("{kind} gate: no image urls available on selected metadata")
        }
    }
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn organize_rationale(
    item: &MediaItem,
    mode: OrganizeMode,
    action: OrganizeAction,
    root: &Path,
    target_dir: &Path,
) -> Vec<String> {
    let mut rationale = vec![
        format!("mode: {:?}", mode),
        format!("action: {:?}", action),
        format!("library root: {}", root.display()),
        format!("target dir: {}", target_dir.display()),
    ];
    if let Some(title) = item.preferred_title() {
        rationale.push(format!(
            "title authority: {} ({:?}, {:.2})",
            title.value, title.origin, title.confidence
        ));
        rationale.push(format!("title reason: {}", title.reason));
    }
    if let Some(year) = item.preferred_year() {
        rationale.push(format!(
            "year authority: {} ({:?}, {:.2})",
            year.value, year.origin, year.confidence
        ));
    }
    if item.media_type == MediaType::TvShow
        && let Some(season) = item.preferred_season()
    {
        rationale.push(format!(
            "season bucket: Season {:02} ({:?}, {:.2})",
            season.value, season.origin, season.confidence
        ));
    }
    rationale
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::OrganizeConfig;
    use crate::models::media::{
        ConfirmationState, IdentityCandidate, IdentityResolution, MediaItem, MediaType,
        ParseSource, ParsedInfo, ScrapeResult, ScrapeSource,
    };

    fn confirmed_identity(title: &str, year: Option<u16>) -> IdentityResolution {
        IdentityResolution {
            confirmation_state: ConfirmationState::Confirmed,
            best: Some(IdentityCandidate {
                source: ScrapeSource::Tmdb,
                title: title.into(),
                year,
                season: None,
                episode: None,
                episode_title: None,
                score: 0.97,
                evidence: vec!["fixture identity confirmed".into()],
            }),
            candidates: Vec::new(),
            evidence_refs: vec!["fixture".into()],
            risk_flags: Vec::new(),
        }
    }

    fn make_org_config() -> OrganizeConfig {
        OrganizeConfig {
            mode: OrganizeMode::Archive,
            root: std::path::PathBuf::from("/tmp/medio_test"),
            link_mode: LinkMode::None,
            with_nfo: false,
            with_images: false,
            cleanup_empty_dirs: false,
        }
    }

    fn make_movie_item(title: &str) -> MediaItem {
        MediaItem {
            id: 0,
            path: std::path::PathBuf::from(format!("/tmp/source/{title}.mp4")),
            file_size: 1024,
            media_type: MediaType::Movie,
            extension: "mp4".into(),
            parsed: Some(ParsedInfo {
                raw_title: title.into(),
                year: Some(2024),
                season: None,
                episode: None,
                resolution: None,
                codec: None,
                source: None,
                release_group: None,
                media_suffix: None,
                parse_source: ParseSource::Regex,
                confidence: 0.9,
                evidence: vec!["test fixture parsed movie".into()],
            }),
            scraped: Some({
                let mut result = ScrapeResult::empty(ScrapeSource::Tmdb, title)
                    .with_confidence(0.9)
                    .with_evidence(["test fixture scraped movie"]);
                result.year = Some(2024);
                result.rating = Some(8.0);
                result.poster_url = Some("https://example.com/poster.jpg".into());
                result.tmdb_id = Some(123);
                result
            }),
            content_evidence: None,
            identity_resolution: Some(confirmed_identity(title, Some(2024))),
            hash: None,
            quality: None,
            rename_plan: None,
        }
    }

    fn make_tv_item(title: &str, season: u32) -> MediaItem {
        MediaItem {
            id: 0,
            path: std::path::PathBuf::from(format!("/tmp/source/{title}.S{season:02}E01.mp4")),
            file_size: 1024,
            media_type: MediaType::TvShow,
            extension: "mp4".into(),
            parsed: Some(ParsedInfo {
                raw_title: title.into(),
                year: None,
                season: Some(season),
                episode: Some(1),
                resolution: None,
                codec: None,
                source: None,
                release_group: None,
                media_suffix: None,
                parse_source: ParseSource::Regex,
                confidence: 0.92,
                evidence: vec!["test fixture parsed episode".into()],
            }),
            scraped: None,
            content_evidence: None,
            identity_resolution: Some(confirmed_identity(title, None)),
            hash: None,
            quality: None,
            rename_plan: None,
        }
    }

    #[test]
    fn test_archive_mode_movie() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].target.to_string_lossy().contains("Movies"));
        assert!(plans[0].target.to_string_lossy().contains("Inception"));
    }

    #[test]
    fn test_archive_mode_tv_with_season() {
        let organizer = Organizer::new(make_org_config());
        let item = make_tv_item("Breaking Bad", 2);
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].target.to_string_lossy().contains("TV Shows"));
        assert!(plans[0].target.to_string_lossy().contains("Season 02"));
    }

    #[test]
    fn test_rename_mode_same_dir() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Test");
        let plans = organizer.plan(&[item], OrganizeMode::Rename, LinkMode::None);
        // Rename mode keeps same parent directory
        if !plans.is_empty() {
            assert_eq!(plans[0].target.parent(), plans[0].source.parent());
        }
    }

    #[test]
    fn test_symlink_mode() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Test");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::Sym);
        assert!(!plans.is_empty());
        assert_eq!(plans[0].action, OrganizeAction::SymLink);
    }

    #[test]
    fn test_hardlink_mode() {
        let organizer = Organizer::new(make_org_config());
        let item = make_movie_item("Test");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::Hard);
        assert!(!plans.is_empty());
        assert_eq!(plans[0].action, OrganizeAction::HardLink);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Movie: A/B"), "Movie_ A_B");
        assert_eq!(sanitize_filename("Clean Title"), "Clean Title");
        assert_eq!(sanitize_filename("What?<>*"), "What____");
    }

    #[test]
    fn test_nfo_generation() {
        let mut config = make_org_config();
        config.with_nfo = true;
        let organizer = Organizer::new(config);
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].nfo_content.is_some());
        assert_eq!(plans[0].decision.nfo_gate.status, AssetGateStatus::Accepted);
        let nfo = plans[0].nfo_content.as_ref().unwrap();
        assert!(nfo.contains("<movie>"));
        assert!(nfo.contains("<title>Inception</title>"));
    }

    #[test]
    fn test_nfo_generation_blocked_for_low_confidence_guess() {
        let mut config = make_org_config();
        config.with_nfo = true;
        let organizer = Organizer::new(config);
        let mut item = make_movie_item("Inception");
        item.identity_resolution = Some(IdentityResolution {
            confirmation_state: ConfirmationState::InsufficientEvidence,
            best: None,
            candidates: Vec::new(),
            evidence_refs: vec!["fixture weak guess".into()],
            risk_flags: Vec::new(),
        });
        item.scraped = Some(
            ScrapeResult::empty(ScrapeSource::Guess, "Noise")
                .with_confidence(0.45)
                .with_evidence(["low confidence guess"]),
        );
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].nfo_content.is_none());
        assert_eq!(
            plans[0].decision.nfo_gate.status,
            AssetGateStatus::UntrustedSource
        );
        assert!(
            plans[0]
                .rationale
                .iter()
                .any(|line| line.contains("Guess metadata is not trusted"))
        );
    }

    #[test]
    fn test_images_do_not_force_nfo_generation() {
        let mut config = make_org_config();
        config.with_images = true;
        let organizer = Organizer::new(config);
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].nfo_content.is_none());
        assert!(!plans[0].image_urls.is_empty());
    }

    #[test]
    fn test_images_blocked_for_ai_metadata() {
        let mut config = make_org_config();
        config.with_images = true;
        let organizer = Organizer::new(config);
        let mut item = make_movie_item("Inception");
        item.scraped = Some({
            let mut result = ScrapeResult::empty(ScrapeSource::AiAssist, "Inception")
                .with_confidence(0.7)
                .with_evidence(["ai result"]);
            result.poster_url = Some("https://example.com/poster.jpg".into());
            result
        });
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert!(!plans.is_empty());
        assert!(plans[0].image_urls.is_empty());
        assert_eq!(
            plans[0].decision.image_gate.status,
            AssetGateStatus::UntrustedSource
        );
        assert!(
            plans[0]
                .rationale
                .iter()
                .any(|line| line.contains("AiAssist metadata is not trusted"))
        );
    }

    #[test]
    fn test_nfo_generation_blocked_for_ambiguous_identity() {
        let mut config = make_org_config();
        config.with_nfo = true;
        let organizer = Organizer::new(config);
        let mut item = make_movie_item("Inception");
        item.identity_resolution = Some(IdentityResolution {
            confirmation_state: ConfirmationState::AmbiguousCandidates,
            best: Some(IdentityCandidate {
                source: ScrapeSource::Tmdb,
                title: "Inception".into(),
                year: Some(2010),
                season: None,
                episode: None,
                episode_title: None,
                score: 0.78,
                evidence: vec!["fixture ambiguous".into()],
            }),
            candidates: Vec::new(),
            evidence_refs: vec!["fixture ambiguous".into()],
            risk_flags: vec!["title collision".into()],
        });

        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].nfo_content.is_none());
        assert_eq!(
            plans[0].decision.nfo_gate.status,
            AssetGateStatus::IdentityUnconfirmed
        );
        assert!(
            plans[0]
                .rationale
                .iter()
                .any(|line| line.contains("identity confirmation is not strong enough"))
        );
    }

    #[test]
    fn test_nfo_generation_allows_high_confidence_candidate_above_stricter_threshold() {
        let mut config = make_org_config();
        config.with_nfo = true;
        let organizer = Organizer::new(config);
        let mut item = make_movie_item("Arrival");
        item.identity_resolution = Some(IdentityResolution {
            confirmation_state: ConfirmationState::HighConfidenceCandidate,
            best: Some(IdentityCandidate {
                source: ScrapeSource::Tmdb,
                title: "Arrival".into(),
                year: Some(2016),
                season: None,
                episode: None,
                episode_title: None,
                score: 0.9,
                evidence: vec!["fixture high confidence".into()],
            }),
            candidates: Vec::new(),
            evidence_refs: vec!["fixture high confidence".into()],
            risk_flags: Vec::new(),
        });

        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].nfo_content.is_some());
        assert_eq!(plans[0].decision.nfo_gate.status, AssetGateStatus::Accepted);
        assert_eq!(plans[0].decision.nfo_gate.threshold, Some(0.92));
    }

    #[test]
    fn test_preflight_detects_existing_target_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let target_root = dir.path().join("library");
        let target_dir = target_root.join("Movies").join("Inception");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("Inception.mp4"), b"existing").unwrap();

        let mut config = make_org_config();
        config.root = target_root;
        let organizer = Organizer::new(config);
        let item = make_movie_item("Inception");
        let plans = organizer.plan(&[item], OrganizeMode::Archive, LinkMode::None);

        assert_eq!(plans.len(), 1);
        assert!(
            plans[0]
                .conflicts
                .iter()
                .any(|msg| msg.contains("already exists"))
        );
    }
}
