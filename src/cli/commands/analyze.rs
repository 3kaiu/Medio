use crate::cli::report::{AnalysisDiagnostic, AnalysisSummary, AnalyzeCommandReport};
use crate::core::config::AppConfig;
use crate::core::pipeline::{Pipeline, PipelineState, ProbeBackend};
use crate::engine::deduplicator::Deduplicator;
use crate::engine::organizer::Organizer;
use crate::engine::renamer::Renamer;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool, probe_backend: &str) {
    let target = Path::new(path);
    let pipeline = Pipeline::new(config);
    let mut state = match pipeline.load_or_scan(target) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    if state.items.is_empty() {
        println!("No media files found.");
        return;
    }

    pipeline.identify(&mut state);
    pipeline.infer_context(&mut state);
    pipeline.hash(&mut state);
    pipeline.probe(&mut state, ProbeBackend::from_cli(probe_backend));
    let rt = match crate::core::runtime::build() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };
    rt.block_on(async {
        pipeline.scrape(&mut state).await;
    });

    let target_path = state.items[0].path.clone();
    let deduplicator = Deduplicator::new(config.dedup.clone());
    let duplicate_groups = deduplicator
        .analyze(&state.items)
        .into_iter()
        .filter(|group| {
            group
                .items
                .iter()
                .any(|entry| state.items[entry.index].path == target_path)
        })
        .collect::<Vec<_>>();

    let renamer = Renamer::new(config.rename.clone());
    let rename_plan = renamer
        .plan(&state.items)
        .into_iter()
        .find(|plan| plan.old_path == target_path);

    let organizer = Organizer::new(config.organize.clone());
    let organize_plans = organizer
        .plan(
            &state.items,
            config.organize.mode,
            config.organize.link_mode,
        )
        .into_iter()
        .filter(|plan| plan.source == target_path)
        .collect::<Vec<_>>();

    let item = &state.items[0];
    let diagnostics = vec![
        AnalysisDiagnostic::identify(item),
        AnalysisDiagnostic::scrape(item),
        AnalysisDiagnostic::dedup(&duplicate_groups, item),
        AnalysisDiagnostic::rename(rename_plan.as_ref()),
        AnalysisDiagnostic::organize(&organize_plans),
    ];
    if json_output {
        let json = serde_json::to_string_pretty(&AnalyzeCommandReport::new(
            &state.stages,
            item,
            &duplicate_groups,
            rename_plan.as_ref(),
            &organize_plans,
            AnalysisSummary::new(
                &state.stages,
                &duplicate_groups,
                rename_plan.as_ref(),
                &organize_plans,
            ),
            diagnostics,
        ))
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_analysis(
            item,
            &state,
            &duplicate_groups,
            rename_plan.as_ref(),
            &organize_plans,
        );
    }
}

fn print_analysis(
    item: &crate::models::media::MediaItem,
    state: &PipelineState,
    duplicate_groups: &[crate::engine::deduplicator::DuplicateGroup],
    rename_plan: Option<&crate::models::media::RenamePlan>,
    organize_plans: &[crate::engine::organizer::OrganizePlan],
) {
    use console::style;

    println!("{}", style("═══ Media Analysis ═══").bold().cyan());
    println!();

    println!("{}", style("Pipeline").bold().yellow());
    println!("  Source:       {}", state.item_source);
    for stage in &state.stages {
        println!("  Stage:        {}", stage.stage);
        for detail in &stage.details {
            println!("    - {detail}");
        }
    }
    let diagnostics = vec![
        AnalysisDiagnostic::identify(item),
        AnalysisDiagnostic::scrape(item),
        AnalysisDiagnostic::dedup(duplicate_groups, item),
        AnalysisDiagnostic::rename(rename_plan),
        AnalysisDiagnostic::organize(organize_plans),
    ];
    println!("  Diagnostics:");
    for diag in diagnostics {
        println!("    * {}: {}", diag.stage, diag.decision);
        for risk in diag.risks.iter().take(2) {
            println!("      risk: {risk}");
        }
    }
    println!();

    // File info
    println!("{}", style("File").bold().yellow());
    println!("  Path:  {}", item.path.display());
    println!("  Size:  {} bytes", item.file_size);
    println!("  Type:  {:?}", item.media_type);
    println!();

    // Parsed info
    if let Some(parsed) = &item.parsed {
        println!("{}", style("Parsed").bold().yellow());
        println!("  Title:        {}", parsed.raw_title);
        if let Some(y) = parsed.year {
            println!("  Year:         {y}");
        }
        if let Some(s) = parsed.season {
            println!("  Season:       {s}");
        }
        if let Some(e) = parsed.episode {
            println!("  Episode:      {e}");
        }
        if let Some(r) = &parsed.resolution {
            println!("  Resolution:   {r}");
        }
        if let Some(c) = &parsed.codec {
            println!("  Codec:        {c}");
        }
        if let Some(g) = &parsed.release_group {
            println!("  Release:      {g}");
        }
        println!("  Parser:       {:?}", parsed.parse_source);
        println!("  Confidence:   {:.2}", parsed.confidence);
        for detail in &parsed.evidence {
            println!("  Evidence:     {detail}");
        }
        println!();
    }

    // Hash info
    if let Some(hash) = &item.hash {
        println!("{}", style("Hash").bold().yellow());
        if let Some(p) = hash.prefix_hash {
            println!("  Prefix: {p:016x}");
        }
        if let Some(f) = hash.full_hash {
            println!("  Full:   {f:016x}");
        }
        println!();
    }

    // Quality info
    if let Some(q) = &item.quality {
        println!("{}", style("Quality").bold().yellow());
        println!("  Resolution:   {}", q.resolution_label);
        if let Some(w) = q.width {
            println!("  Width:        {w}");
        }
        if let Some(h) = q.height {
            println!("  Height:       {h}");
        }
        if let Some(vc) = &q.video_codec {
            println!("  Video Codec:  {vc}");
        }
        if let Some(ac) = &q.audio_codec {
            println!("  Audio Codec:  {ac}");
        }
        if let Some(d) = q.duration_secs {
            println!("  Duration:     {}s", d);
        }
        println!("  Score:        {:.1}", q.quality_score);
        println!();
    }

    // Scraped info
    if let Some(s) = &item.scraped {
        println!("{}", style("Scraped").bold().green());
        println!("  Source:       {:?}", s.source);
        println!("  Title:        {}", s.title);
        println!("  Confidence:   {:.2}", s.confidence);
        if let Some(y) = s.year {
            println!("  Year:         {y}");
        }
        if let Some(r) = s.rating {
            println!("  Rating:       {r:.1}");
        }
        if let Some(en) = &s.episode_name {
            println!("  Episode:      {en}");
        }
        if let Some(a) = &s.artist {
            println!("  Artist:       {a}");
        }
        if let Some(al) = &s.album {
            println!("  Album:        {al}");
        }
        if let Some(au) = &s.author {
            println!("  Author:       {au}");
        }
        for detail in &s.evidence {
            println!("  Evidence:     {detail}");
        }
        println!();
    } else {
        println!("{}", style("Scraped: — (no metadata found)").dim());
        println!();
    }

    if let Some(content) = &item.content_evidence {
        println!("{}", style("Content Probe").bold().yellow());
        if let Some(title) = &content.container.title {
            println!("  Container:    {title}");
        }
        if !content.container.chapters.is_empty() {
            println!("  Chapters:     {}", content.container.chapters.join(" | "));
        }
        if !content.title_candidates.is_empty() {
            println!("  Titles:       {}", content.title_candidates.join(" | "));
        }
        println!("  Subtitles:    {}", content.subtitles.len());
        if !content.season_hypotheses.is_empty() || !content.episode_hypotheses.is_empty() {
            println!(
                "  Hints:        season={:?} episode={:?}",
                content.season_hypotheses, content.episode_hypotheses
            );
        }
        for risk in &content.risk_flags {
            println!("  Risk:         {risk}");
        }
        println!();
    }

    if let Some(identity) = &item.identity_resolution {
        println!("{}", style("Identity").bold().yellow());
        println!("  State:        {:?}", identity.confirmation_state);
        if let Some(best) = &identity.best {
            println!("  Best:         {} ({:.2})", best.title, best.score);
            if let Some(year) = best.year {
                println!("  Year:         {year}");
            }
            if let Some(season) = best.season {
                println!("  Season:       {season}");
            }
            if let Some(episode) = best.episode {
                println!("  Episode:      {episode}");
            }
            if let Some(name) = &best.episode_title {
                println!("  Episode Name: {name}");
            }
        }
        for risk in &identity.risk_flags {
            println!("  Risk:         {risk}");
        }
        println!();
    }

    if !duplicate_groups.is_empty() {
        println!("{}", style("Dedup").bold().yellow());
        for group in duplicate_groups {
            println!("  Group:        {} ({:?})", group.content_id, group.kind);
            println!("  Strategy:     {}", group.keep_strategy);
            println!("  Summary:      {}", group.summary);
            for guard in &group.guardrails {
                println!("  Guard:        {guard}");
            }
            for entry in &group.items {
                let group_item = &state.items[entry.index];
                println!(
                    "    {} {}",
                    if entry.is_keep { "KEEP" } else { "DROP" },
                    group_item.path.display()
                );
                println!("      Why:      {}", entry.rationale);
                if !entry.basis.is_empty() {
                    println!("      Basis:    {}", entry.basis.join(", "));
                }
            }
        }
        println!();
    }

    if let Some(plan) = rename_plan {
        println!("{}", style("Rename").bold().yellow());
        println!("  Target:       {}", plan.new_path.display());
        for reason in &plan.rationale {
            println!("  Why:          {reason}");
        }
        if plan.conflicts.is_empty() {
            println!("  Conflicts:    none");
        } else {
            for conflict in &plan.conflicts {
                println!("  Conflict:     {conflict}");
            }
        }
        for sub in &plan.subtitle_plans {
            println!(
                "  Subtitle:     {} → {}",
                sub.old_path.display(),
                sub.new_path.display()
            );
        }
        for dir in &plan.directory_plans {
            println!(
                "  Dir Rename:   {} → {}",
                dir.old_path.display(),
                dir.new_path.display()
            );
        }
        println!();
    }

    if !organize_plans.is_empty() {
        println!("{}", style("Organize").bold().yellow());
        for plan in organize_plans {
            println!("  Action:       {:?}", plan.action);
            println!("  Target:       {}", plan.target.display());
            for reason in &plan.rationale {
                println!("  Why:          {reason}");
            }
            println!(
                "  Assets:       nfo={} images={}",
                if plan.nfo_content.is_some() {
                    "yes"
                } else {
                    "no"
                },
                plan.image_urls.len()
            );
            if plan.conflicts.is_empty() {
                println!("  Conflicts:    none");
            } else {
                for conflict in &plan.conflicts {
                    println!("  Conflict:     {conflict}");
                }
            }
        }
        println!();
    }
}
