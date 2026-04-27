use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::engine::renamer::Renamer;
use crate::scraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, dry_run: bool, json_output: bool) {
    let root = Path::new(path);
    if !root.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

    // Step 1: Load scan index or scan live
    let mut items = super::load_scan_items_or_scan(root, config);

    if items.is_empty() {
        println!("No media files found.");
        return;
    }

    let keyword_filter = KeywordFilter::new(config.scan.keyword_filter.clone());
    let identifier = Identifier::new(keyword_filter);
    identifier.parse_batch(&mut items);

    for item in items.iter_mut() {
        ContextInfer::enrich_item(item);
    }

    // Step 2: Scrape metadata so renaming can use authoritative titles
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        scraper::populate_scrape_results(&mut items, config).await;
    });

    // Step 3: Generate rename plans
    let renamer = Renamer::new(config.rename.clone());
    let plans = renamer.plan(&items);

    if plans.is_empty() {
        println!("No files need renaming.");
        return;
    }

    // Output plans
    if json_output {
        let json = serde_json::to_string_pretty(&plans)
            .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_rename_table(&plans);
    }

    // Step 4: Execute or preview
    let is_dry = dry_run || config.general.dry_run;

    if !is_dry && config.general.confirm {
        if !dialoguer::Confirm::new()
            .with_prompt(format!("{} files will be renamed. Proceed?", plans.len()))
            .default(false)
            .interact()
            .unwrap_or(false)
        {
            println!("Aborted.");
            return;
        }
    }

    let actions = renamer.execute(&plans, is_dry);
    for action in &actions {
        println!("{action}");
    }

    println!(
        "\n{} rename plans generated, {} actions taken.",
        plans.len(),
        actions.len()
    );
}

fn print_rename_table(plans: &[crate::models::media::RenamePlan]) {
    use console::style;

    println!(
        "{}  {}  {}",
        style("Old").bold().cyan().dim(),
        style("→").bold().yellow().dim(),
        style("New").bold().green().dim(),
    );

    for plan in plans {
        let old_name = plan
            .old_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let new_name = plan
            .new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        println!(
            "  {} → {}",
            super::truncate(&old_name, 50),
            super::truncate(&new_name, 50)
        );

        for sub in &plan.subtitle_plans {
            let sub_old = sub
                .old_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let sub_new = sub
                .new_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            println!(
                "  {} → {}  (subtitle)",
                super::truncate(&sub_old, 48),
                super::truncate(&sub_new, 48)
            );
        }
    }
}
