use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::models::media::MediaItem;
use crate::scraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool, with_scrape: bool) {
    let root = Path::new(path);
    if !root.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }
    if !root.is_dir() {
        eprintln!("Error: path is not a directory: {path}");
        return;
    }

    // Step 1: Scan
    let scanner = Scanner::new(config.scan.clone());
    let mut items = scanner.scan(root);

    if items.is_empty() {
        println!("No media files found.");
        return;
    }

    // Step 2: Keyword filter + Identify
    let keyword_filter = KeywordFilter::new(config.scan.keyword_filter.clone());
    let identifier = Identifier::new(keyword_filter);
    identifier.parse_batch(&mut items);

    // Step 3: Context inference (parent dir)
    for item in items.iter_mut() {
        if let Some(parsed) = &item.parsed {
            let parent_dirs = ContextInfer::collect_parent_dirs(&item.path, 3);
            let inferred = ContextInfer::infer(parsed, &parent_dirs);
            item.parsed = Some(inferred);
        }
    }

    // Step 4: Optional scrape
    if with_scrape {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            scraper::populate_scrape_results(&mut items, config).await;
        });
    }

    // Output
    if json_output {
        let json = serde_json::to_string_pretty(&items).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_scan_table(&items);
    }
}


fn print_scan_table(items: &[MediaItem]) {
    use console::style;
    use indicatif::HumanBytes;

    println!(
        "{}  {}  {}  {}  {}",
        style("Type").bold().cyan().dim(),
        style("Title").bold().cyan().dim(),
        style("Year").bold().cyan().dim(),
        style("S/E").bold().cyan().dim(),
        style("Size").bold().cyan().dim(),
    );

    for item in items {
        let type_str = format!("{:?}", item.media_type);
        let (title, year, season_ep) = if let Some(p) = &item.parsed {
            let y = p.year.map(|y| y.to_string()).unwrap_or_default();
            let se = match (p.season, p.episode) {
                (Some(s), Some(e)) => format!("S{s:02}E{e:02}"),
                (Some(s), None) => format!("S{s:02}"),
                _ => String::new(),
            };
            (p.raw_title.clone(), y, se)
        } else {
            ("—".into(), String::new(), String::new())
        };

        let size = HumanBytes(item.file_size);

        println!(
            "{:<8} {:<40} {:<6} {:<8} {}",
            type_str,
            super::truncate(&title, 40),
            year,
            season_ep,
            size,
        );
    }

    println!("\n{} media files found", items.len());
}
