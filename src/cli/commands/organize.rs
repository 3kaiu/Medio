use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::core::types::{LinkMode, OrganizeMode};
use crate::engine::organizer::Organizer;
use crate::scraper::local;
use crate::scraper::musicbrainz::MusicBrainzScraper;
use crate::scraper::openlibrary::OpenLibraryScraper;
use crate::scraper::tmdb::TmdbScraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, mode: &str, with_nfo: bool, with_images: bool, link: &str, dry_run: bool, json_output: bool) {
    let root = Path::new(path);
    if !root.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

    // Parse mode
    let organize_mode = match mode.to_lowercase().as_str() {
        "rename" => OrganizeMode::Rename,
        "archive" => OrganizeMode::Archive,
        "local" => OrganizeMode::Local,
        _ => {
            eprintln!("Unknown mode: {mode} (use: rename, archive, local)");
            return;
        }
    };

    let link_mode = match link.to_lowercase().as_str() {
        "none" | "" => LinkMode::None,
        "hard" => LinkMode::Hard,
        "sym" | "symlink" => LinkMode::Sym,
        _ => LinkMode::None,
    };

    // Override config with CLI flags
    let mut org_config = config.organize.clone();
    if with_nfo { org_config.with_nfo = true; }
    if with_images { org_config.with_images = true; }

    // Step 1: Scan + Identify
    let scanner = Scanner::new(config.scan.clone());
    let mut items = scanner.scan(root);

    if items.is_empty() {
        println!("No media files found.");
        return;
    }

    let keyword_filter = KeywordFilter::new(config.scan.keyword_filter.clone());
    let identifier = Identifier::new(keyword_filter);
    identifier.parse_batch(&mut items);

    for item in items.iter_mut() {
        if let Some(parsed) = &item.parsed {
            let parent_dirs = collect_parent_dirs(&item.path, 3);
            let inferred = ContextInfer::infer(parsed, &parent_dirs);
            item.parsed = Some(inferred);
        }
    }

    // Step 2: Scrape metadata for better organization
    let tmdb = TmdbScraper::new(&config.api);
    let mb = MusicBrainzScraper::new(&config.api);
    let ol = OpenLibraryScraper::new();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for item in items.iter_mut() {
            // Try local NFO first
            if let Some(nfo_path) = local::find_nfo(&item.path) {
                if let Some(result) = local::read_nfo(&nfo_path) {
                    item.scraped = Some(result);
                    continue;
                }
            }

            let result = match item.media_type {
                crate::models::media::MediaType::Movie | crate::models::media::MediaType::TvShow => {
                    if let Some(parsed) = &item.parsed {
                        tmdb.scrape(parsed, &item.media_type).await.ok().flatten()
                    } else {
                        None
                    }
                }
                crate::models::media::MediaType::Music => {
                    if let Some(parsed) = &item.parsed {
                        mb.search_recording(
                            parsed.raw_title.split('.').next().unwrap_or(&parsed.raw_title),
                            &parsed.raw_title,
                        ).await.ok().flatten()
                    } else {
                        None
                    }
                }
                crate::models::media::MediaType::Novel => {
                    if let Some(parsed) = &item.parsed {
                        ol.search(&parsed.raw_title, None).await.ok().flatten()
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if result.is_some() {
                item.scraped = result;
            }
        }
    });

    // Step 3: Generate organize plans
    let organizer = Organizer::new(org_config);
    let plans = organizer.plan(&items, organize_mode, link_mode);

    if plans.is_empty() {
        println!("All files are already organized.");
        return;
    }

    // Output plans
    if json_output {
        let json = serde_json::to_string_pretty(&plans.iter().map(|p| serde_json::json!({
            "source": p.source,
            "target": p.target,
            "action": format!("{:?}", p.action),
            "nfo": p.nfo_content.is_some(),
            "images": p.image_urls.len(),
        })).collect::<Vec<_>>()).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_organize_table(&plans);
    }

    // Step 4: Execute
    let is_dry = dry_run || config.general.dry_run;
    let actions = organizer.execute(&plans, is_dry);

    for action in &actions {
        println!("{action}");
    }

    println!("\n{} plans, {} actions.", plans.len(), actions.len());
}

fn collect_parent_dirs(path: &std::path::Path, max: usize) -> Vec<&std::path::Path> {
    let mut dirs = Vec::new();
    let mut current = path.parent();
    while let Some(dir) = current {
        if dirs.len() >= max { break; }
        dirs.push(dir);
        current = dir.parent();
    }
    dirs
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
        let src_name = plan.source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let tgt_dir = plan.target.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let mut extras: Vec<String> = Vec::new();
        if plan.nfo_content.is_some() { extras.push("nfo".into()); }
        if !plan.image_urls.is_empty() { extras.push(format!("{}img", plan.image_urls.len())); }

        println!(
            "  {:<10} {} → {}/  {}",
            action,
            truncate(&src_name, 35),
            truncate(&tgt_dir, 35),
            extras.join("+"),
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}
