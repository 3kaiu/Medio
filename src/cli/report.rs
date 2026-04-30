use crate::core::pipeline::StageReport;
use crate::engine::deduplicator::DuplicateGroup;
use crate::engine::execution_report::ExecutionReport;
use crate::engine::organizer::OrganizePlan;
use crate::models::media::{MediaItem, MetadataOrigin, RenamePlan};
use serde::Serialize;

pub const CLI_SCHEMA_VERSION: &str = "1";
pub const KIND_PIPELINE_REPORT: &str = "pipeline_report";
pub const KIND_EXECUTION_REPORT: &str = "execution_report";
pub const KIND_ANALYSIS_REPORT: &str = "analysis_report";
pub const COMMAND_SCAN: &str = "scan";
pub const COMMAND_SCRAPE: &str = "scrape";
pub const COMMAND_ANALYZE: &str = "analyze";
pub const COMMAND_RENAME: &str = "rename";
pub const COMMAND_DEDUP: &str = "dedup";
pub const COMMAND_ORGANIZE: &str = "organize";

#[derive(Serialize)]
pub struct PipelineSummary {
    pub stage_count: usize,
    pub item_count: usize,
    pub scraped_items: usize,
}

impl PipelineSummary {
    pub fn new(stage_count: usize, item_count: usize, scraped_items: usize) -> Self {
        Self {
            stage_count,
            item_count,
            scraped_items,
        }
    }
}

#[derive(Serialize)]
pub struct AnalysisSummary {
    pub stage_count: usize,
    pub duplicate_groups: usize,
    pub guarded_duplicate_groups: usize,
    pub rename_planned: bool,
    pub rename_blocked: bool,
    pub organize_plans: usize,
    pub organize_blocked: usize,
    pub organize_nfo_ready: usize,
    pub organize_image_ready: usize,
}

impl AnalysisSummary {
    pub fn new(
        stages: &[StageReport],
        duplicate_groups: &[DuplicateGroup],
        rename_plan: Option<&RenamePlan>,
        organize_plans: &[OrganizePlan],
    ) -> Self {
        Self {
            stage_count: stages.len(),
            duplicate_groups: duplicate_groups.len(),
            guarded_duplicate_groups: duplicate_groups
                .iter()
                .filter(|group| !group.guardrails.is_empty())
                .count(),
            rename_planned: rename_plan.is_some(),
            rename_blocked: rename_plan
                .map(|plan| !plan.conflicts.is_empty())
                .unwrap_or(false),
            organize_plans: organize_plans.len(),
            organize_blocked: organize_plans
                .iter()
                .filter(|plan| !plan.conflicts.is_empty())
                .count(),
            organize_nfo_ready: organize_plans
                .iter()
                .filter(|plan| plan.nfo_content.is_some())
                .count(),
            organize_image_ready: organize_plans
                .iter()
                .filter(|plan| !plan.image_urls.is_empty())
                .count(),
        }
    }
}

#[derive(Serialize)]
pub struct AnalysisDiagnostic {
    pub stage: &'static str,
    pub decision: String,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
}

impl AnalysisDiagnostic {
    pub fn identify(item: &MediaItem) -> Self {
        match &item.parsed {
            Some(parsed) => {
                let mut evidence = Vec::new();
                evidence.push(format!("parse_source={:?}", parsed.parse_source));
                evidence.push(format!("confidence={:.2}", parsed.confidence));
                if !parsed.raw_title.is_empty() {
                    evidence.push(format!("title={}", parsed.raw_title));
                }
                evidence.extend(parsed.evidence.iter().take(3).cloned());

                let mut risks = Vec::new();
                if parsed.confidence < 0.6 {
                    risks.push(format!(
                        "parser confidence is low ({:.2})",
                        parsed.confidence
                    ));
                }
                if parsed.year.is_none() && parsed.season.is_none() && parsed.episode.is_none() {
                    risks.push("parser did not recover year or episode identity".into());
                }

                Self {
                    stage: "identify",
                    decision: format!("parsed metadata accepted from {:?}", parsed.parse_source),
                    evidence,
                    risks,
                }
            }
            None => Self {
                stage: "identify",
                decision: "no parsed metadata recovered".into(),
                evidence: Vec::new(),
                risks: vec!["downstream stages rely on weak filename identity".into()],
            },
        }
    }

    pub fn scrape(item: &MediaItem) -> Self {
        let mut evidence = Vec::new();
        let mut risks = Vec::new();
        if let Some(content) = &item.content_evidence {
            evidence.push(format!(
                "content_titles={}",
                content.title_candidates.join(" | ")
            ));
            evidence.push(format!("subtitle_sources={}", content.subtitles.len()));
            evidence.extend(content.risk_flags.iter().take(2).cloned());
        }
        if let Some(resolution) = &item.identity_resolution {
            evidence.push(format!(
                "confirmation_state={:?}",
                resolution.confirmation_state
            ));
            if let Some(best) = &resolution.best {
                evidence.push(format!("best_candidate={} ({:.2})", best.title, best.score));
            }
            risks.extend(resolution.risk_flags.iter().take(3).cloned());
        }
        match &item.scraped {
            Some(scraped) => {
                let authority = scraped.authority_score();
                evidence.push(format!("source={:?}", scraped.source));
                evidence.push(format!("confidence={:.2}", scraped.confidence));
                evidence.push(format!("authority={:.2}", authority));
                evidence.push(format!("title={}", scraped.title));
                evidence.extend(scraped.evidence.iter().take(3).cloned());

                if authority < 0.72 {
                    risks.push(format!(
                        "scraped authority below trusted override threshold ({:.2})",
                        authority
                    ));
                }
                if matches!(
                    scraped.source,
                    crate::models::media::ScrapeSource::Guess
                        | crate::models::media::ScrapeSource::AiAssist
                ) {
                    risks.push(format!(
                        "{:?} metadata may be insufficient for asset automation",
                        scraped.source
                    ));
                }

                Self {
                    stage: "scrape",
                    decision: format!("scraped metadata selected from {:?}", scraped.source),
                    evidence,
                    risks,
                }
            }
            None => Self {
                stage: "scrape",
                decision: "no scraped metadata selected".into(),
                evidence,
                risks: if risks.is_empty() {
                    vec!["rename and organize fall back to parsed-only identity".into()]
                } else {
                    risks
                },
            },
        }
    }

    pub fn dedup(groups: &[DuplicateGroup], item: &MediaItem) -> Self {
        if groups.is_empty() {
            return Self {
                stage: "dedup",
                decision: "item is not part of any duplicate group".into(),
                evidence: vec![format!("path={}", item.path.display())],
                risks: Vec::new(),
            };
        }

        let mut evidence = Vec::new();
        let mut risks = Vec::new();
        for group in groups.iter().take(3) {
            evidence.push(format!(
                "{}:{:?}:drops={}",
                group.content_id, group.kind, group.decision.drop_count
            ));
            if group.decision.guarded {
                risks.extend(group.guardrails.iter().cloned());
            }
        }

        Self {
            stage: "dedup",
            decision: format!(
                "{} duplicate group(s) relevant; {} guarded",
                groups.len(),
                groups.iter().filter(|group| group.decision.guarded).count()
            ),
            evidence,
            risks,
        }
    }

    pub fn rename(plan: Option<&RenamePlan>) -> Self {
        match plan {
            Some(plan) => {
                let mut evidence = vec![
                    format!("target={}", plan.new_path.display()),
                    format!("template={}", plan.decision.template),
                    format!("subtitle_count={}", plan.decision.subtitle_count),
                    format!("directory_count={}", plan.decision.directory_count),
                ];
                if let Some(origin) = plan.decision.title_origin {
                    evidence.push(format!("title_origin={}", metadata_origin_label(origin)));
                }

                let mut risks = plan.conflicts.clone();
                if plan.decision.title_confidence.unwrap_or(0.0) < 0.6 {
                    risks.push("rename title confidence is low".into());
                }

                Self {
                    stage: "rename",
                    decision: if plan.conflicts.is_empty() {
                        "rename plan is executable".into()
                    } else {
                        "rename plan is blocked by conflicts".into()
                    },
                    evidence,
                    risks,
                }
            }
            None => Self {
                stage: "rename",
                decision: "no rename needed".into(),
                evidence: Vec::new(),
                risks: Vec::new(),
            },
        }
    }

    pub fn organize(plans: &[OrganizePlan]) -> Self {
        if plans.is_empty() {
            return Self {
                stage: "organize",
                decision: "no organize action planned".into(),
                evidence: Vec::new(),
                risks: Vec::new(),
            };
        }

        let mut evidence = Vec::new();
        let mut risks = Vec::new();
        for plan in plans.iter().take(3) {
            evidence.push(format!(
                "{:?}:target={}:nfo={:?}:image={:?}",
                plan.action,
                plan.target.display(),
                plan.decision.nfo_gate.status,
                plan.decision.image_gate.status
            ));
            risks.extend(plan.conflicts.iter().cloned());
        }

        Self {
            stage: "organize",
            decision: format!(
                "{} organize plan(s), {} blocked",
                plans.len(),
                plans
                    .iter()
                    .filter(|plan| !plan.conflicts.is_empty())
                    .count()
            ),
            evidence,
            risks,
        }
    }
}

fn metadata_origin_label(origin: MetadataOrigin) -> &'static str {
    match origin {
        MetadataOrigin::Parsed => "parsed",
        MetadataOrigin::Scraped => "scraped",
    }
}

#[derive(Serialize)]
pub struct ExecutionSummary {
    pub entry_count: usize,
    pub ready_entries: usize,
    pub blocked_entries: usize,
    pub guarded_entries: usize,
}

impl ExecutionSummary {
    pub fn from_rename_plans(plans: &[RenamePlan]) -> Self {
        let blocked_entries = plans
            .iter()
            .filter(|plan| !plan.conflicts.is_empty())
            .count();
        Self {
            entry_count: plans.len(),
            ready_entries: plans.len().saturating_sub(blocked_entries),
            blocked_entries,
            guarded_entries: 0,
        }
    }

    pub fn from_organize_plans(plans: &[OrganizePlan]) -> Self {
        let blocked_entries = plans
            .iter()
            .filter(|plan| !plan.conflicts.is_empty())
            .count();
        Self {
            entry_count: plans.len(),
            ready_entries: plans.len().saturating_sub(blocked_entries),
            blocked_entries,
            guarded_entries: 0,
        }
    }

    pub fn from_duplicate_groups(groups: &[DuplicateGroup]) -> Self {
        let guarded_entries = groups
            .iter()
            .filter(|group| !group.guardrails.is_empty())
            .count();
        Self {
            entry_count: groups.len(),
            ready_entries: groups.len().saturating_sub(guarded_entries),
            blocked_entries: 0,
            guarded_entries,
        }
    }
}

#[derive(Serialize)]
pub struct ScanCommandReport<'a, T> {
    pub schema_version: &'static str,
    pub kind: &'static str,
    pub command: &'static str,
    pub root: String,
    pub item_source: &'a str,
    pub summary: PipelineSummary,
    pub processed: bool,
    pub scraped: bool,
    pub stages: &'a [StageReport],
    pub items: &'a [T],
}

impl<'a, T> ScanCommandReport<'a, T> {
    pub fn new(
        root: String,
        item_source: &'a str,
        processed: bool,
        scraped: bool,
        stages: &'a [StageReport],
        items: &'a [T],
    ) -> Self {
        Self {
            schema_version: CLI_SCHEMA_VERSION,
            kind: KIND_PIPELINE_REPORT,
            command: COMMAND_SCAN,
            root,
            item_source,
            summary: PipelineSummary::new(
                stages.len(),
                items.len(),
                if scraped { items.len() } else { 0 },
            ),
            processed,
            scraped,
            stages,
            items,
        }
    }
}

#[derive(Serialize)]
pub struct ScrapeCommandReport<'a, T> {
    pub schema_version: &'static str,
    pub kind: &'static str,
    pub command: &'static str,
    pub root: String,
    pub item_source: &'a str,
    pub summary: PipelineSummary,
    pub stages: &'a [StageReport],
    pub items: &'a [T],
    pub scraped_count: usize,
}

impl<'a, T> ScrapeCommandReport<'a, T> {
    pub fn new(
        root: String,
        item_source: &'a str,
        stages: &'a [StageReport],
        items: &'a [T],
        scraped_count: usize,
    ) -> Self {
        Self {
            schema_version: CLI_SCHEMA_VERSION,
            kind: KIND_PIPELINE_REPORT,
            command: COMMAND_SCRAPE,
            root,
            item_source,
            summary: PipelineSummary::new(stages.len(), items.len(), scraped_count),
            stages,
            items,
            scraped_count,
        }
    }
}

#[derive(Serialize)]
pub struct ExecutionCommandReport<'a, T> {
    pub schema_version: &'static str,
    pub kind: &'static str,
    pub command: &'static str,
    pub summary: ExecutionSummary,
    pub entries: &'a [T],
    pub report: &'a ExecutionReport,
    pub dry_run: bool,
    pub aborted: bool,
}

impl<'a, T> ExecutionCommandReport<'a, T> {
    pub fn new(
        command: &'static str,
        summary: ExecutionSummary,
        entries: &'a [T],
        report: &'a ExecutionReport,
        dry_run: bool,
        aborted: bool,
    ) -> Self {
        Self {
            schema_version: CLI_SCHEMA_VERSION,
            kind: KIND_EXECUTION_REPORT,
            command,
            summary,
            entries,
            report,
            dry_run,
            aborted,
        }
    }
}

#[derive(Serialize)]
pub struct AnalyzeCommandReport<'a, TItem, TGroup, TPlan, TOrganize> {
    pub schema_version: &'static str,
    pub kind: &'static str,
    pub command: &'static str,
    pub summary: AnalysisSummary,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub stages: &'a [StageReport],
    pub item: &'a TItem,
    pub duplicate_groups: &'a [TGroup],
    pub rename_plan: Option<&'a TPlan>,
    pub organize_plans: &'a [TOrganize],
}

impl<'a, TItem, TGroup, TPlan, TOrganize>
    AnalyzeCommandReport<'a, TItem, TGroup, TPlan, TOrganize>
{
    pub fn new(
        stages: &'a [StageReport],
        item: &'a TItem,
        duplicate_groups: &'a [TGroup],
        rename_plan: Option<&'a TPlan>,
        organize_plans: &'a [TOrganize],
        summary: AnalysisSummary,
        diagnostics: Vec<AnalysisDiagnostic>,
    ) -> Self {
        Self {
            schema_version: CLI_SCHEMA_VERSION,
            kind: KIND_ANALYSIS_REPORT,
            command: COMMAND_ANALYZE,
            summary,
            diagnostics,
            stages,
            item,
            duplicate_groups,
            rename_plan,
            organize_plans,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_stage() -> Vec<StageReport> {
        vec![StageReport {
            stage: "identify".into(),
            item_count: 1,
            details: vec!["parsed_items: 1".into()],
        }]
    }

    fn sample_report() -> ExecutionReport {
        let mut report = ExecutionReport::new("rename");
        report.executed = 1;
        report.details.push("[dry-run] RENAME /a -> /b".into());
        report
    }

    #[test]
    fn scan_report_serializes_stable_envelope() {
        let stages = sample_stage();
        let items = [json!({"path":"/media/a.mkv"})];
        let value = serde_json::to_value(ScanCommandReport::new(
            "/media".into(),
            "live_scan",
            true,
            false,
            &stages,
            &items,
        ))
        .unwrap();

        assert_eq!(value["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(value["kind"], KIND_PIPELINE_REPORT);
        assert_eq!(value["command"], COMMAND_SCAN);
        assert_eq!(value["root"], "/media");
        assert_eq!(value["item_source"], "live_scan");
        assert_eq!(value["summary"]["stage_count"], 1);
        assert_eq!(value["summary"]["item_count"], 1);
        assert_eq!(value["summary"]["scraped_items"], 0);
        assert_eq!(value["processed"], true);
        assert_eq!(value["scraped"], false);
        assert!(value.get("stages").is_some());
        assert!(value.get("items").is_some());
    }

    #[test]
    fn scrape_report_serializes_stable_envelope() {
        let stages = sample_stage();
        let items = [json!({"title":"Example"})];
        let value = serde_json::to_value(ScrapeCommandReport::new(
            "/media".into(),
            "cached_index",
            &stages,
            &items,
            1,
        ))
        .unwrap();

        assert_eq!(value["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(value["kind"], KIND_PIPELINE_REPORT);
        assert_eq!(value["command"], COMMAND_SCRAPE);
        assert_eq!(value["root"], "/media");
        assert_eq!(value["item_source"], "cached_index");
        assert_eq!(value["summary"]["stage_count"], 1);
        assert_eq!(value["summary"]["item_count"], 1);
        assert_eq!(value["summary"]["scraped_items"], 1);
        assert_eq!(value["scraped_count"], 1);
        assert!(value.get("stages").is_some());
        assert!(value.get("items").is_some());
    }

    #[test]
    fn analyze_report_serializes_stable_envelope() {
        let stages = sample_stage();
        let item = json!({"path":"/media/a.mkv"});
        let duplicate_groups = [json!({"content_id":"show|S1E1"})];
        let rename_plan = json!({"new_path":"/media/b.mkv"});
        let organize_plans = [json!({"target":"/library/b.mkv"})];
        let value = serde_json::to_value(AnalyzeCommandReport::new(
            &stages,
            &item,
            &duplicate_groups,
            Some(&rename_plan),
            &organize_plans,
            AnalysisSummary {
                stage_count: 1,
                duplicate_groups: 1,
                guarded_duplicate_groups: 0,
                rename_planned: true,
                rename_blocked: false,
                organize_plans: 1,
                organize_blocked: 0,
                organize_nfo_ready: 0,
                organize_image_ready: 0,
            },
            vec![AnalysisDiagnostic {
                stage: "rename",
                decision: "rename plan is executable".into(),
                evidence: vec!["target=/media/b.mkv".into()],
                risks: Vec::new(),
            }],
        ))
        .unwrap();

        assert_eq!(value["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(value["kind"], KIND_ANALYSIS_REPORT);
        assert_eq!(value["command"], COMMAND_ANALYZE);
        assert_eq!(value["summary"]["duplicate_groups"], 1);
        assert_eq!(value["summary"]["rename_planned"], true);
        assert_eq!(value["diagnostics"][0]["stage"], "rename");
        assert!(value.get("stages").is_some());
        assert!(value.get("item").is_some());
        assert!(value.get("duplicate_groups").is_some());
        assert!(value.get("rename_plan").is_some());
        assert!(value.get("organize_plans").is_some());
    }

    #[test]
    fn execution_report_serializes_stable_commands() {
        let entries = [json!({"target":"/library/b.mkv"})];
        let report = sample_report();

        for command in [COMMAND_RENAME, COMMAND_DEDUP, COMMAND_ORGANIZE] {
            let value = serde_json::to_value(ExecutionCommandReport::new(
                command,
                ExecutionSummary {
                    entry_count: 1,
                    ready_entries: 1,
                    blocked_entries: 0,
                    guarded_entries: 0,
                },
                &entries,
                &report,
                true,
                false,
            ))
            .unwrap();

            assert_eq!(value["schema_version"], CLI_SCHEMA_VERSION);
            assert_eq!(value["kind"], KIND_EXECUTION_REPORT);
            assert_eq!(value["command"], command);
            assert_eq!(value["summary"]["entry_count"], 1);
            assert_eq!(value["dry_run"], true);
            assert_eq!(value["aborted"], false);
            assert!(value.get("entries").is_some());
            assert!(value.get("report").is_some());
        }
    }
}
