use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::hasher::FileHasher;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::db::cache::Cache;
use crate::media::ffprobe::FfprobeProbe;
use crate::media::native_probe::NativeProbe;
use crate::media::probe::MediaProbe;
use crate::scraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool, probe_backend: &str) {
    let target = Path::new(path);
    if !target.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

    // Single file or directory
    let mut items = if target.is_file() {
        let scanner = Scanner::new(config.scan.clone());
        scanner
            .scan(target.parent().unwrap_or(Path::new(".")))
            .into_iter()
            .filter(|i| i.path == target)
            .collect()
    } else {
        let scanner = Scanner::new(config.scan.clone());
        scanner.scan(target)
    };

    if items.is_empty() {
        println!("No media files found.");
        return;
    }

    // Step 1: Identify
    let keyword_filter = KeywordFilter::new(config.scan.keyword_filter.clone());
    let identifier = Identifier::new(keyword_filter);
    identifier.parse_batch(&mut items);

    // Step 2: Context inference
    ContextInfer::enrich_item(&mut items[0]);

    // Step 3: Hash
    let cache = Cache::open(&config.cache_path()).ok();
    if let Some(ref cache) = cache {
        let _ = cache.cleanup(config.cache.ttl_days);
    }
    FileHasher::compute_all_with_cache(&mut items, cache.as_ref());

    // Step 4: Probe quality
    {
        let item = &mut items[0];
        let use_ffprobe = if probe_backend == "ffprobe" {
            FfprobeProbe::is_available()
        } else if probe_backend == "native" {
            false
        } else {
            !config.general.dry_run && FfprobeProbe::is_available()
        };
        if use_ffprobe {
            let probe = FfprobeProbe::new(config.quality.clone());
            if let Ok(quality) = probe.probe(&item.path) {
                item.quality = Some(quality);
            }
        } else {
            let probe = NativeProbe::new(config.quality.clone());
            if let Ok(quality) = probe.probe(&item.path) {
                item.quality = Some(quality);
            }
        }
    }

    // Step 5: Scrape using the shared fallback chain + cache + AI path
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        scraper::populate_scrape_results(&mut items, config).await;
    });

    // Output
    let item = &items[0];
    if json_output {
        let json = serde_json::to_string_pretty(&item)
            .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_analysis(item);
    }
}

fn print_analysis(item: &crate::models::media::MediaItem) {
    use console::style;

    println!("{}", style("═══ Media Analysis ═══").bold().cyan());
    println!();

    // File info
    println!("{}", style("File").bold().yellow());
    println!("  Path:  {}", item.path.display());
    println!("  Size:  {} bytes", item.file_size);
    println!("  Type:  {:?}", item.media_type);
    println!();

    // Parsed info
    if let Some(parsed) = &item.parsed {
        println!("{}", style("Parsed").bold().yellow());
        println!("  Title:        {}", parsed.raw_title);
        if let Some(y) = parsed.year {
            println!("  Year:         {y}");
        }
        if let Some(s) = parsed.season {
            println!("  Season:       {s}");
        }
        if let Some(e) = parsed.episode {
            println!("  Episode:      {e}");
        }
        if let Some(r) = &parsed.resolution {
            println!("  Resolution:   {r}");
        }
        if let Some(c) = &parsed.codec {
            println!("  Codec:        {c}");
        }
        if let Some(g) = &parsed.release_group {
            println!("  Release:      {g}");
        }
        println!();
    }

    // Hash info
    if let Some(hash) = &item.hash {
        println!("{}", style("Hash").bold().yellow());
        if let Some(p) = hash.prefix_hash {
            println!("  Prefix: {p:016x}");
        }
        if let Some(f) = hash.full_hash {
            println!("  Full:   {f:016x}");
        }
        println!();
    }

    // Quality info
    if let Some(q) = &item.quality {
        println!("{}", style("Quality").bold().yellow());
        println!("  Resolution:   {}", q.resolution_label);
        if let Some(w) = q.width {
            println!("  Width:        {w}");
        }
        if let Some(h) = q.height {
            println!("  Height:       {h}");
        }
        if let Some(vc) = &q.video_codec {
            println!("  Video Codec:  {vc}");
        }
        if let Some(ac) = &q.audio_codec {
            println!("  Audio Codec:  {ac}");
        }
        if let Some(d) = q.duration_secs {
            println!("  Duration:     {}s", d);
        }
        println!("  Score:        {:.1}", q.quality_score);
        println!();
    }

    // Scraped info
    if let Some(s) = &item.scraped {
        println!("{}", style("Scraped").bold().green());
        println!("  Source:       {:?}", s.source);
        println!("  Title:        {}", s.title);
        if let Some(y) = s.year {
            println!("  Year:         {y}");
        }
        if let Some(r) = s.rating {
            println!("  Rating:       {r:.1}");
        }
        if let Some(en) = &s.episode_name {
            println!("  Episode:      {en}");
        }
        if let Some(a) = &s.artist {
            println!("  Artist:       {a}");
        }
        if let Some(al) = &s.album {
            println!("  Album:        {al}");
        }
        if let Some(au) = &s.author {
            println!("  Author:       {au}");
        }
        println!();
    } else {
        println!("{}", style("Scraped: — (no metadata found)").dim());
        println!();
    }
}
