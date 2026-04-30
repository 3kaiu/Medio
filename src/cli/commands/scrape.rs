use crate::cli::report::ScrapeCommandReport;
use crate::core::config::AppConfig;
use crate::core::pipeline::Pipeline;
use crate::models::media::MediaItem;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool) {
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

    if json_output {
        let scraped_count = state.items.iter().filter(|i| i.scraped.is_some()).count();
        let json = serde_json::to_string_pretty(&ScrapeCommandReport::new(
            state.root.display().to_string(),
            &state.item_source,
            &state.stages,
            &state.items,
            scraped_count,
        ))
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_scrape_table(&state.items);
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
