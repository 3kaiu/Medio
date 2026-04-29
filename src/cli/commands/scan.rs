use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::db::cache::Cache;
use crate::models::media::{MediaItem, ScanIndex};
use crate::scraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool, process: bool, with_scrape: bool) {
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

    persist_scan_index(config, root, &items);

    let process = process || with_scrape;

    if process {
        // Step 2: Keyword filter + Identify
        let keyword_filter = KeywordFilter::new(config.scan.keyword_filter.clone());
        let identifier = Identifier::new(keyword_filter);
        identifier.parse_batch(&mut items);

        // Step 3: Context inference (parent dir)
        for item in items.iter_mut() {
            ContextInfer::enrich_item(item);
        }

        // Step 4: Optional scrape
        if with_scrape {
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
        }
    }

    // Output
    if json_output {
        let json = serde_json::to_string_pretty(&items)
            .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else if process {
        print_processed_scan_table(&items);
    } else {
        print_structure_table(root, &items);
    }
}

fn persist_scan_index(config: &AppConfig, root: &Path, items: &[MediaItem]) {
    let cache_path = config.cache_path();
    let Some(root_key) = root.to_str() else {
        return;
    };

    let Ok(cache) = Cache::open(&cache_path) else {
        return;
    };

    let index = ScanIndex {
        root: root.to_path_buf(),
        items: items.to_vec(),
    };

    if cache.set_scan_index(root_key, &index).is_ok() {
        let _ = cache.flush();
    }
}

fn print_structure_table(root: &Path, items: &[MediaItem]) {
    use console::style;
    use indicatif::HumanBytes;

    println!(
        "{}  {}  {}  {}",
        style("Kind").bold().cyan().dim(),
        style("Ext").bold().cyan().dim(),
        style("Size").bold().cyan().dim(),
        style("Path").bold().cyan().dim(),
    );

    for item in items {
        let kind = match item.media_type {
            crate::models::media::MediaType::Movie | crate::models::media::MediaType::TvShow => {
                "Video"
            }
            crate::models::media::MediaType::Music => "Music",
            crate::models::media::MediaType::Novel => "Novel",
            crate::models::media::MediaType::Strm => "Strm",
            crate::models::media::MediaType::Unknown => "Unknown",
        };
        let path = item
            .path
            .strip_prefix(root)
            .unwrap_or(&item.path)
            .display()
            .to_string();
        let size = HumanBytes(item.file_size);

        println!("{:<8} {:<6} {:<10} {}", kind, item.extension, size, path);
    }

    println!("\n{} media files found", items.len());
}

fn print_processed_scan_table(items: &[MediaItem]) {
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
                (None, Some(e)) => format!("E{e:02}"),
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
