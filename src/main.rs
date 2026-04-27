mod ai;
mod cli;
mod core;
mod db;
mod engine;
mod media;
mod models;
mod scraper;
mod tui;

use clap::Parser;
use cli::args::{Cli, Commands};
use core::config::AppConfig;

fn main() {
    let cli = Cli::parse();

    // Init tracing
    let level = if cli.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("medio={level}"))
        .init();

    // Load config
    let mut config = AppConfig::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config: {e}, using defaults");
        AppConfig::default()
    });

    // Apply CLI flags to config
    if cli.no_ai {
        config.ai.enabled = false;
    }
    if cli.concurrency != config.ai.concurrency {
        config.ai.concurrency = cli.concurrency;
    }

    core::oplog::init(config.general.operation_log);

    match cli.command {
        Commands::Scan {
            path,
            process,
            with_scrape,
        } => {
            cli::commands::scan::run(&path, &config, cli.json, process, with_scrape);
        }
        Commands::Scrape { path } => {
            cli::commands::scrape::run(&path, &config, cli.json);
        }
        Commands::Rename { path } => {
            cli::commands::rename::run(&path, &config, cli.dry_run, cli.json);
        }
        Commands::Dedup { path } => {
            // CLI --dry-run=false means user wants to execute, but config default is dry_run=true
            // So actual dry_run = cli.dry_run (from flag, default true)
            cli::commands::dedup::run(&path, &config, cli.dry_run, cli.json, &cli.probe);
        }
        Commands::Organize {
            path,
            mode,
            with_nfo,
            with_images,
            link,
        } => {
            cli::commands::organize::run(
                &path,
                &config,
                &mode,
                with_nfo,
                with_images,
                &link,
                cli.dry_run,
                cli.json,
            );
        }
        Commands::Analyze { path } => {
            cli::commands::analyze::run(&path, &config, cli.json, &cli.probe);
        }
        Commands::Tui => {
            cli::commands::tui::run(".", &config);
        }
        Commands::Config { init } => {
            let path = AppConfig::config_path();
            if init {
                cli::commands::config::run_init(&path);
            } else {
                println!("Config file: {}", path.display());
                if !path.exists() {
                    let default_config = AppConfig::default();
                    match default_config.save() {
                        Ok(()) => println!("Default config created at {}", path.display()),
                        Err(e) => eprintln!("Error creating config: {e}"),
                    }
                } else {
                    // Show current config summary
                    let config = AppConfig::load().unwrap_or_default();
                    println!(
                        "  TMDB key: {}",
                        if config.api.tmdb_key.is_empty() {
                            "not set"
                        } else {
                            "configured"
                        }
                    );
                    println!(
                        "  MusicBrainz: {}",
                        if config.api.musicbrainz_user_agent.is_empty() {
                            "not set"
                        } else {
                            "configured"
                        }
                    );
                    println!("  AI provider: {:?}", config.ai.provider);
                    println!("  AI enabled: {}", config.ai.enabled);
                    println!("  Organize mode: {:?}", config.organize.mode);
                    println!("  Dry run: {}", config.general.dry_run);
                    println!("  Operation log: {}", config.general.operation_log);
                }
            }
        }
    }
}
