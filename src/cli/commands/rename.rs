use crate::cli::report::{COMMAND_RENAME, ExecutionCommandReport, ExecutionSummary};
use crate::core::config::AppConfig;
use crate::core::pipeline::Pipeline;
use crate::engine::renamer::Renamer;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, dry_run: bool, json_output: bool) {
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

    let renamer = Renamer::new(config.rename.clone());
    let plans = renamer.plan(&state.items);

    if plans.is_empty() {
        println!("No files need renaming.");
        return;
    }

    if !json_output {
        crate::cli::render::print_rename_table(&plans);
    }

    let is_dry = dry_run || config.general.dry_run;
    let blocked = plans
        .iter()
        .filter(|plan| !plan.conflicts.is_empty())
        .count();

    if !is_dry
        && config.general.confirm
        && !dialoguer::Confirm::new()
            .with_prompt(format!(
                "{} rename plans ready, {} blocked by conflicts. Proceed?",
                plans.len().saturating_sub(blocked),
                blocked
            ))
            .default(false)
            .interact()
            .unwrap_or(false)
    {
        if json_output {
            let report = crate::engine::execution_report::ExecutionReport::new("rename");
            let json = serde_json::to_string_pretty(&ExecutionCommandReport::new(
                COMMAND_RENAME,
                ExecutionSummary::from_rename_plans(&plans),
                &plans,
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

    let report = renamer.execute_report(&plans, is_dry);
    if json_output {
        let json = serde_json::to_string_pretty(&ExecutionCommandReport::new(
            COMMAND_RENAME,
            ExecutionSummary::from_rename_plans(&plans),
            &plans,
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
