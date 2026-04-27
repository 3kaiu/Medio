use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::models::media::MediaItem;
use crate::scraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool) {
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

    // Step 2: Scrape metadata using fallback chain (concurrent)
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        scraper::populate_scrape_results(&mut items, config).await;
    });

    // Output
    if json_output {
        let json = serde_json::to_string_pretty(&items)
            .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_scrape_table(&items);
    }
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
            (
                s.title.clone(),
                format!("{:?}", s.source),
                s.rating.map(|r| format!("{r:.1}")).unwrap_or_default(),
            )
        } else {
            ("—".into(), "—".into(), String::new())
        };

        println!(
            "{:<8} {:<40} {:<40} {:<12} {}",
            format!("{:?}", item.media_type),
            super::truncate(
                &item
                    .parsed
                    .as_ref()
                    .map(|p| p.raw_title.clone())
                    .unwrap_or_default(),
                40
            ),
            super::truncate(&scraped_title, 40),
            source,
            rating,
        );
    }

    let scraped_count = items.iter().filter(|i| i.scraped.is_some()).count();
    println!("\n{}/{} files scraped", scraped_count, items.len());
}
