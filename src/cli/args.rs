use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "medio",
    version,
    about = "Media file manager: rename, deduplicate, organize"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Only preview, don't execute
    #[arg(long)]
    pub dry_run: bool,

    /// Ask confirmation before each action
    #[arg(long)]
    pub confirm: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Verbose debug logging
    #[arg(long)]
    pub debug: bool,

    /// Media probe backend
    #[arg(long, default_value = "native")]
    pub probe: String,

    /// Disable AI assistance
    #[arg(long)]
    pub no_ai: bool,

    /// Concurrency limit
    #[arg(long, default_value_t = 3)]
    pub concurrency: usize,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan media files and record directory structure
    Scan {
        /// Directory to scan
        path: String,

        /// Parse filenames and infer series context during scan
        #[arg(long)]
        process: bool,

        /// Also scrape metadata during scan
        #[arg(long)]
        with_scrape: bool,
    },

    /// Scrape metadata (NFO + images)
    Scrape {
        /// Directory to scrape
        path: String,
    },

    /// Rename files (default: --dry-run)
    Rename {
        /// Directory to rename
        path: String,
    },

    /// Deduplicate files (default: --dry-run)
    Dedup {
        /// Directory to deduplicate
        path: String,
    },

    /// Organize files (default: --dry-run)
    Organize {
        /// Directory to organize
        path: String,

        /// Organization mode
        #[arg(long, default_value = "archive")]
        mode: String,

        /// Generate NFO during organize
        #[arg(long)]
        with_nfo: bool,

        /// Download images during organize
        #[arg(long)]
        with_images: bool,

        /// Link mode: none/hard/sym
        #[arg(long, default_value = "none")]
        link: String,
    },

    /// Analyze a single file
    Analyze {
        /// File to analyze
        path: String,
    },

    /// Interactive TUI
    Tui,

    /// Show config status or initialize config
    Config {
        /// Initialize config with interactive wizard
        #[arg(long)]
        init: bool,
    },
}
