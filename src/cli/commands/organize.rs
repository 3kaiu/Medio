use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::types::{LinkMode, OrganizeMode};
use crate::engine::organizer::Organizer;
use crate::engine::renamer::Renamer;
use crate::scraper;
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
    if !root.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

    // Parse mode
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

    // Override config with CLI flags
    let mut org_config = config.organize.clone();
    if options.with_nfo {
        org_config.with_nfo = true;
    }
    if options.with_images {
        org_config.with_images = true;
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

    // Step 2: Scrape metadata for better organization
    let rt = match crate::core::runtime::build() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };
    rt.block_on(async {
        scraper::populate_scrape_results(&mut items, config).await;
    });

    if organize_mode == OrganizeMode::Rename {
        let renamer = Renamer::new(config.rename.clone());
        let plans = renamer.plan(&items);

        if plans.is_empty() {
            println!("No files need renaming.");
            return;
        }

        if options.json_output {
            let json = serde_json::to_string_pretty(&plans)
                .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
            println!("{json}");
        } else {
            print_rename_table(&plans);
        }

        let is_dry = options.dry_run || config.general.dry_run;
        if !is_dry
            && config.general.confirm
            && !dialoguer::Confirm::new()
                .with_prompt(format!("{} files will be renamed. Proceed?", plans.len()))
                .default(false)
                .interact()
                .unwrap_or(false)
        {
            println!("Aborted.");
            return;
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
        return;
    }

    // Step 3: Generate organize plans
    let organizer = Organizer::new(org_config);
    let plans = organizer.plan(&items, organize_mode, link_mode);

    if plans.is_empty() {
        println!("All files are already organized.");
        return;
    }

    // Output plans
    if options.json_output {
        let json = serde_json::to_string_pretty(
            &plans
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "source": p.source,
                        "target": p.target,
                        "action": format!("{:?}", p.action),
                        "nfo": p.nfo_content.is_some(),
                        "images": p.image_urls.len(),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_organize_table(&plans);
    }

    // Step 4: Execute
    let is_dry = options.dry_run || config.general.dry_run;
    if !is_dry
        && config.general.confirm
        && !dialoguer::Confirm::new()
            .with_prompt(format!(
                "{} organize actions will be applied. Proceed?",
                plans.len()
            ))
            .default(false)
            .interact()
            .unwrap_or(false)
    {
        println!("Aborted.");
        return;
    }

    let actions = organizer.execute(&plans, is_dry);
    for action in &actions {
        println!("{action}");
    }

    println!("\n{} plans, {} actions.", plans.len(), actions.len());
}

fn print_organize_table(plans: &[crate::engine::organizer::OrganizePlan]) {
    use console::style;

    println!(
        "{}  {}  {}  {}  {}",
        style("Action").bold().cyan().dim(),
        style("Source").bold().cyan().dim(),
        style("→").bold().yellow().dim(),
        style("Target").bold().green().dim(),
        style("Extras").bold().cyan().dim(),
    );

    for plan in plans {
        let action = format!("{:?}", plan.action).to_lowercase();
        let src_name = plan
            .source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let tgt_dir = plan
            .target
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut extras: Vec<String> = Vec::new();
        if plan.nfo_content.is_some() {
            extras.push("nfo".into());
        }
        if !plan.image_urls.is_empty() {
            extras.push(format!("{}img", plan.image_urls.len()));
        }

        println!(
            "  {:<10} {} → {}/  {}",
            action,
            super::truncate(&src_name, 35),
            super::truncate(&tgt_dir, 35),
            extras.join("+"),
        );
    }
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
