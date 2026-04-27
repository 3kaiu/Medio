use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::hasher::FileHasher;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::db::cache::Cache;
use crate::engine::deduplicator::Deduplicator;
use crate::media::ffprobe::FfprobeProbe;
use crate::media::native_probe::NativeProbe;
use crate::media::probe::MediaProbe;
use crate::models::media::MediaItem;
use indicatif::HumanBytes;
use rayon::prelude::*;
use std::path::Path;

pub fn run(path: &str, config: &AppConfig, dry_run: bool, json_output: bool, probe_backend: &str) {
    let root = Path::new(path);
    if !root.exists() {
        eprintln!("Error: path does not exist: {path}");
        return;
    }

    // Step 1: Scan
    let scanner = Scanner::new(config.scan.clone());
    let mut items = scanner.scan(root);

    if items.is_empty() {
        println!("No media files found.");
        return;
    }

    // Step 2: Identify
    let keyword_filter = KeywordFilter::new(config.scan.keyword_filter.clone());
    let identifier = Identifier::new(keyword_filter);
    identifier.parse_batch(&mut items);

    // Step 3: Context inference
    for item in items.iter_mut() {
        if let Some(parsed) = &item.parsed {
            let parent_dirs = ContextInfer::collect_parent_dirs(&item.path, 3);
            let inferred = ContextInfer::infer(parsed, &parent_dirs);
            item.parsed = Some(inferred);
        }
    }

    // Step 4: Hash
    println!("Computing hashes...");
    let cache = Cache::open(&config.cache_path()).ok();
    if let Some(ref cache) = cache {
        let _ = cache.cleanup(config.cache.ttl_days);
    }
    FileHasher::compute_all_with_cache(&mut items, cache.as_ref());

    // Step 5: Probe quality
    println!("Probing media quality...");
    let use_ffprobe = if probe_backend == "ffprobe" {
        FfprobeProbe::is_available()
    } else if probe_backend == "native" {
        false
    } else {
        !config.general.dry_run && FfprobeProbe::is_available()
    };

    if use_ffprobe {
        let probe = FfprobeProbe::new(config.quality.clone());
        probe_items(&mut items, &probe);
    } else {
        let probe = NativeProbe::new(config.quality.clone());
        probe_items(&mut items, &probe);
    }

    // Step 6: Dedup analysis
    let deduplicator = Deduplicator::new(config.dedup.clone());
    let groups = deduplicator.analyze(&items);

    if groups.is_empty() {
        println!("No duplicates found.");
        return;
    }

    // Print results
    if json_output {
        let json = serde_json::to_string_pretty(&groups).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
    } else {
        print_dedup_table(&groups, &items);
    }

    // Step 7: Execute or preview
    let is_dry = dry_run || config.general.dry_run;
    if !is_dry && config.general.confirm {
        let pending_actions = groups
            .iter()
            .map(|group| group.items.iter().filter(|item| !item.is_keep).count())
            .sum::<usize>();
        if !dialoguer::Confirm::new()
            .with_prompt(format!("{} duplicate files will be removed. Proceed?", pending_actions))
            .default(false)
            .interact()
            .unwrap_or(false)
        {
            println!("Aborted.");
            return;
        }
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let actions = rt.block_on(deduplicator.execute(&groups, &items, is_dry)).unwrap_or_else(|e| vec![format!("Error: {e}")]);

    for action in &actions {
        println!("{action}");
    }

    let removed = actions.iter().filter(|a| !a.starts_with("[error]") && !a.starts_with("[skip]")).count();
    println!("\n{} files processed, {} actions taken.", groups.iter().map(|g| g.items.len()).sum::<usize>(), removed);
}

fn probe_items(items: &mut [MediaItem], probe: &dyn MediaProbe) {
    items.par_iter_mut().for_each(|item| {
        if let Ok(quality) = probe.probe(&item.path) {
            item.quality = Some(quality);
        }
    });
}


fn print_dedup_table(groups: &[crate::engine::deduplicator::DuplicateGroup], items: &[MediaItem]) {
    use console::style;

    for (gi, group) in groups.iter().enumerate() {
        println!("\n{}", style(format!("Group {} — {}", gi + 1, group.content_id)).bold().yellow());

        println!(
            "  {}  {}  {}  {}  {}",
            style("Keep").bold().cyan().dim(),
            style("Path").bold().cyan().dim(),
            style("Size").bold().cyan().dim(),
            style("Quality").bold().cyan().dim(),
            style("Score").bold().cyan().dim(),
        );

        for di in &group.items {
            let item = &items[di.index];
            let keep = if di.is_keep { "✓ KEEP" } else { "✗ REMOVE" };
            let score = if let Some(q) = &item.quality {
                format!("{:.1}", q.quality_score)
            } else {
                "—".into()
            };
            let quality = item.quality.as_ref().map(|q| q.resolution_label.clone()).unwrap_or_default();

            println!(
                "  {:<10} {:<50} {:<10} {:<10} {}",
                keep,
                super::truncate(&item.path.display().to_string(), 50),
                HumanBytes(item.file_size).to_string(),
                quality,
                score,
            );
        }
    }
}
