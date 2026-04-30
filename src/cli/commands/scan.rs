use crate::cli::report::ScanCommandReport;
use crate::core::config::AppConfig;
use crate::core::pipeline::Pipeline;
use crate::models::media::MediaItem;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool, process: bool, with_scrape: bool) {
    let root = Path::new(path);
    let pipeline = Pipeline::new(config);
    let mut state = match pipeline.scan_root(root) {
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

    let process = process || with_scrape;

    if process {
        pipeline.identify(&mut state);
        pipeline.infer_context(&mut state);

        if with_scrape {
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
        }
    }

    if json_output {
        let json = serde_json::to_string_pretty(&ScanCommandReport::new(
            state.root.display().to_string(),
            &state.item_source,
            process,
            with_scrape,
            &state.stages,
            &state.items,
        ))
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else if process {
        print_processed_scan_table(&state.items);
    } else {
        print_structure_table(root, &state.items);
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
