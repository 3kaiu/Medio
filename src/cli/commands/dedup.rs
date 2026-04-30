use crate::cli::report::{COMMAND_DEDUP, ExecutionCommandReport, ExecutionSummary};
use crate::core::config::AppConfig;
use crate::core::pipeline::{Pipeline, ProbeBackend};
use crate::engine::deduplicator::Deduplicator;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, dry_run: bool, json_output: bool, probe_backend: &str) {
    let root = Path::new(path);
    let pipeline = Pipeline::new(config);
    let mut state = match pipeline.load_or_scan(root) {
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

    println!("Computing hashes...");
    pipeline.hash(&mut state);

    println!("Probing media quality...");
    pipeline.probe(&mut state, ProbeBackend::from_cli(probe_backend));

    let deduplicator = Deduplicator::new(config.dedup.clone());
    let groups = deduplicator.analyze(&state.items);

    if groups.is_empty() {
        println!("No duplicates found.");
        return;
    }

    if !json_output {
        crate::cli::render::print_dedup_table(&groups, &state.items);
    }

    let is_dry = dry_run || config.general.dry_run;
    if !is_dry && config.general.confirm {
        let pending_actions = groups
            .iter()
            .map(|group| group.items.iter().filter(|item| !item.is_keep).count())
            .sum::<usize>();
        let guarded_groups = groups
            .iter()
            .filter(|group| !group.guardrails.is_empty())
            .count();
        if !dialoguer::Confirm::new()
            .with_prompt(format!(
                "{} duplicate files scheduled, {} groups guarded for review. Proceed?",
                pending_actions, guarded_groups
            ))
            .default(false)
            .interact()
            .unwrap_or(false)
        {
            if json_output {
                let report = crate::engine::execution_report::ExecutionReport::new("dedup");
                let json = serde_json::to_string_pretty(&ExecutionCommandReport::new(
                    COMMAND_DEDUP,
                    ExecutionSummary::from_duplicate_groups(&groups),
                    &groups,
                    &report,
                    is_dry,
                    true,
                ))
                .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
                println!("{json}");
                return;
            }
            println!("Aborted.");
            return;
        }
    }

    let rt = match crate::core::runtime::build() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };
    let report = rt
        .block_on(deduplicator.execute_report(&groups, &state.items, is_dry))
        .unwrap_or_else(|e| {
            let mut report = crate::engine::execution_report::ExecutionReport::new("dedup");
            report.errors = 1;
            report.details.push(format!("Error: {e}"));
            report
        });
    if json_output {
        let json = serde_json::to_string_pretty(&ExecutionCommandReport::new(
            COMMAND_DEDUP,
            ExecutionSummary::from_duplicate_groups(&groups),
            &groups,
            &report,
            is_dry,
            false,
        ))
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
        return;
    }

    for action in &report.details {
        println!("{action}");
    }

    println!("\n{}", report.summary_line());
}
