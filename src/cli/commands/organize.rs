use crate::cli::report::{
    COMMAND_ORGANIZE, COMMAND_RENAME, ExecutionCommandReport, ExecutionSummary,
};
use crate::core::config::AppConfig;
use crate::core::pipeline::Pipeline;
use crate::core::types::{LinkMode, OrganizeMode};
use crate::engine::organizer::Organizer;
use crate::engine::renamer::Renamer;
use std::path::Path;

pub struct OrganizeOptions<'a> {
    pub mode: &'a str,
    pub with_nfo: bool,
    pub with_images: bool,
    pub link: &'a str,
    pub dry_run: bool,
    pub json_output: bool,
}

pub fn run(path: &str, config: &AppConfig, options: OrganizeOptions<'_>) {
    let root = Path::new(path);
    let pipeline = Pipeline::new(config);

    let organize_mode = match options.mode.to_lowercase().as_str() {
        "rename" => OrganizeMode::Rename,
        "archive" => OrganizeMode::Archive,
        "local" => OrganizeMode::Local,
        _ => {
            eprintln!(
                "Unknown mode: {} (use: rename, archive, local)",
                options.mode
            );
            return;
        }
    };

    let link_mode = match options.link.to_lowercase().as_str() {
        "none" | "" => LinkMode::None,
        "hard" => LinkMode::Hard,
        "sym" | "symlink" => LinkMode::Sym,
        _ => LinkMode::None,
    };

    let mut org_config = config.organize.clone();
    if options.with_nfo {
        org_config.with_nfo = true;
    }
    if options.with_images {
        org_config.with_images = true;
    }

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

    if organize_mode == OrganizeMode::Rename {
        let renamer = Renamer::new(config.rename.clone());
        let plans = renamer.plan(&state.items);

        if plans.is_empty() {
            println!("No files need renaming.");
            return;
        }

        if !options.json_output {
            crate::cli::render::print_rename_table(&plans);
        }

        let is_dry = options.dry_run || config.general.dry_run;
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
            if options.json_output {
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
        if options.json_output {
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
        return;
    }

    let organizer = Organizer::new(org_config);
    let plans = organizer.plan(&state.items, organize_mode, link_mode);

    if plans.is_empty() {
        println!("All files are already organized.");
        return;
    }

    if !options.json_output {
        crate::cli::render::print_organize_table(&plans);
    }

    let is_dry = options.dry_run || config.general.dry_run;
    let blocked = plans
        .iter()
        .filter(|plan| !plan.conflicts.is_empty())
        .count();
    let nfo_ready = plans
        .iter()
        .filter(|plan| plan.nfo_content.is_some())
        .count();
    let image_ready = plans
        .iter()
        .filter(|plan| !plan.image_urls.is_empty())
        .count();
    if !is_dry
        && config.general.confirm
        && !dialoguer::Confirm::new()
            .with_prompt(format!(
                "{} organize plans ready, {} blocked, trusted assets: nfo={} image={}. Proceed?",
                plans.len().saturating_sub(blocked),
                blocked,
                nfo_ready,
                image_ready
            ))
            .default(false)
            .interact()
            .unwrap_or(false)
    {
        if options.json_output {
            let report = crate::engine::execution_report::ExecutionReport::new("organize");
            let json = serde_json::to_string_pretty(&ExecutionCommandReport::new(
                COMMAND_ORGANIZE,
                ExecutionSummary::from_organize_plans(&plans),
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

    let report = organizer.execute_report(&plans, is_dry);
    if options.json_output {
        let json = serde_json::to_string_pretty(&ExecutionCommandReport::new(
            COMMAND_ORGANIZE,
            ExecutionSummary::from_organize_plans(&plans),
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
