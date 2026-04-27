use crate::ai::openai_compat::OpenAiCompat;
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
use crate::scraper::local;
use crate::scraper::musicbrainz::MusicBrainzScraper;
use crate::scraper::openlibrary::OpenLibraryScraper;
use crate::scraper::tmdb::TmdbScraper;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, json_output: bool) {
    let target = Path::new(path);
    if !target.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

    // Single file or directory
    let mut items = if target.is_file() {
        let scanner = Scanner::new(config.scan.clone());
        scanner.scan(target.parent().unwrap_or(Path::new(".")))
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
    {
        let item = &mut items[0];
        if let Some(parsed) = &item.parsed {
            let parent_dirs = collect_parent_dirs(&item.path, 3);
            let inferred = ContextInfer::infer(parsed, &parent_dirs);
            item.parsed = Some(inferred);
        }
    }

    // Step 3: Hash
    let cache = Cache::open(&config.cache_path()).ok();
    if let Some(ref cache) = cache {
        let _ = cache.cleanup(config.cache.ttl_days);
    }
    FileHasher::compute_all_with_cache(&mut items, cache.as_ref());

    // Step 4: Probe quality
    {
        let item = &mut items[0];
        let use_ffprobe = !config.general.dry_run && FfprobeProbe::is_available();
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

    // Step 5: Scrape (local NFO + online)
    {
        let item = &mut items[0];
        if let Some(nfo_path) = local::find_nfo(&item.path) {
            if let Some(result) = local::read_nfo(&nfo_path) {
                item.scraped = Some(result);
            }
        }
    }

    {
        let item = &mut items[0];
        if item.scraped.is_none() {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let tmdb = TmdbScraper::new(&config.api);
                let mb = MusicBrainzScraper::new(&config.api);
                let ol = OpenLibraryScraper::new();

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
            });
        }
    }

    // Step 6: AI assist (if enabled and not yet scraped)
    if config.ai.enabled {
        let item = &mut items[0];
        if item.scraped.is_none() {
            let ai = OpenAiCompat::from_config(&config.ai);
            if ai.is_configured() {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let filename = item.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                if let Ok(Some(result)) = rt.block_on(ai.identify(&filename)) {
                    item.scraped = Some(result);
                }
            }
        }
    }

    // Output
    let item = &items[0];
    if json_output {
        let json = serde_json::to_string_pretty(&item).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_analysis(item);
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
        if let Some(y) = parsed.year { println!("  Year:         {y}"); }
        if let Some(s) = parsed.season { println!("  Season:       {s}"); }
        if let Some(e) = parsed.episode { println!("  Episode:      {e}"); }
        if let Some(r) = &parsed.resolution { println!("  Resolution:   {r}"); }
        if let Some(c) = &parsed.codec { println!("  Codec:        {c}"); }
        if let Some(g) = &parsed.release_group { println!("  Release:      {g}"); }
        println!();
    }

    // Hash info
    if let Some(hash) = &item.hash {
        println!("{}", style("Hash").bold().yellow());
        if let Some(p) = hash.prefix_hash { println!("  Prefix: {p:016x}"); }
        if let Some(f) = hash.full_hash { println!("  Full:   {f:016x}"); }
        println!();
    }

    // Quality info
    if let Some(q) = &item.quality {
        println!("{}", style("Quality").bold().yellow());
        println!("  Resolution:   {}", q.resolution_label);
        if let Some(w) = q.width { println!("  Width:        {w}"); }
        if let Some(h) = q.height { println!("  Height:       {h}"); }
        if let Some(vc) = &q.video_codec { println!("  Video Codec:  {vc}"); }
        if let Some(ac) = &q.audio_codec { println!("  Audio Codec:  {ac}"); }
        if let Some(d) = q.duration_secs { println!("  Duration:     {}s", d); }
        println!("  Score:        {:.1}", q.quality_score);
        println!();
    }

    // Scraped info
    if let Some(s) = &item.scraped {
        println!("{}", style("Scraped").bold().green());
        println!("  Source:       {:?}", s.source);
        println!("  Title:        {}", s.title);
        if let Some(y) = s.year { println!("  Year:         {y}"); }
        if let Some(r) = s.rating { println!("  Rating:       {r:.1}"); }
        if let Some(en) = &s.episode_name { println!("  Episode:      {en}"); }
        if let Some(a) = &s.artist { println!("  Artist:       {a}"); }
        if let Some(al) = &s.album { println!("  Album:        {al}"); }
        if let Some(au) = &s.author { println!("  Author:       {au}"); }
        println!();
    } else {
        println!("{}", style("Scraped: — (no metadata found)").dim());
        println!();
    }
}
