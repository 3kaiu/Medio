use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::models::media::MediaItem;
use crate::scraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool) {
    let root = Path::new(path);
    if !root.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

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

    // Step 2: Scrape metadata using fallback chain (concurrent)
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        scraper::populate_scrape_results(&mut items, config).await;
    });

    // Output
    if json_output {
        let json = serde_json::to_string_pretty(&items).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_scrape_table(&items);
    }
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

fn print_scrape_table(items: &[MediaItem]) {
    use console::style;

    println!(
        "{}  {}  {}  {}  {}",
        style("Type").bold().cyan().dim(),
        style("Title").bold().cyan().dim(),
        style("Scraped").bold().cyan().dim(),
        style("Source").bold().cyan().dim(),
        style("Rating").bold().cyan().dim(),
    );

    for item in items {
        let (scraped_title, source, rating) = if let Some(s) = &item.scraped {
            (s.title.clone(), format!("{:?}", s.source), s.rating.map(|r| format!("{r:.1}")).unwrap_or_default())
        } else {
            ("—".into(), "—".into(), String::new())
        };

        println!(
            "{:<8} {:<40} {:<40} {:<12} {}",
            format!("{:?}", item.media_type),
            truncate(&item.parsed.as_ref().map(|p| p.raw_title.clone()).unwrap_or_default(), 40),
            truncate(&scraped_title, 40),
            source,
            rating,
        );
    }

    let scraped_count = items.iter().filter(|i| i.scraped.is_some()).count();
    println!("\n{}/{} files scraped", scraped_count, items.len());
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}
