# Medio

Media file manager: rename, deduplicate, organize.

## Features

- **Scan** — Recursive media file discovery with smart identification (movie/TV/music/book/STRM)
- **Dedup** — Progressive xxHash deduplication with quality-aware keep strategy
- **Scrape** — Metadata from TMDB, MusicBrainz, OpenLibrary, local NFO, and AI
- **Rename** — Template-based renaming with subtitle file tracking
- **Analyze** — Single-file deep analysis (hash, quality probe, scrape, AI)
- **Organize** — Archive/local/rename modes with NFO generation and image download
- **TUI** — Interactive terminal UI with search, tabs, and detail view

## Quick Start

Install via Homebrew

```
brew tap 3kaiu/medio
brew install medio
```

Or via script

```
# Optional: -s latest for main branch, -s 0.1.0 for specific version
curl -fsSL https://raw.githubusercontent.com/3kaiu/Medio/main/install.sh | bash
```

Or from source

```
cargo install medio --git https://github.com/3kaiu/Medio
```

## Usage

```
me                          # Interactive TUI
me scan /path/to/media      # Scan and identify media files
me dedup /path/to/media     # Deduplicate files (dry-run by default)
me scrape /path/to/media    # Scrape metadata (NFO + images)
me rename /path/to/media    # Rename files (dry-run by default)
me analyze /path/to/file    # Analyze a single file
me organize /path/to/media  # Organize into library structure
me config                   # Show config status
me config --init             # Interactive config wizard
me --version                 # Show version
me --help                    # Show help
```

Preview safely

```
me dedup /path --dry-run=false    # Execute dedup (default is dry-run)
me rename /path --dry-run=false   # Execute rename
me organize /path --mode archive --with-nfo --with-images
me organize /path --mode local --link sym   # Symlink instead of move
me --json scan /path              # JSON output for piping
```

Short alias: `me` = `medio`

## Configuration

```
me config          # Show config status
me config --init   # Interactive wizard (TMDB key, AI, organize root)
```

Config file: `~/.config/medio/config.toml`

Key settings:

- `api.tmdb_key` — TMDB API key for movie/TV metadata
- `api.musicbrainz_user_agent` — MusicBrainz user agent
- `ai.deepseek.key` — DeepSeek API key for AI-assisted identification
- `rename.movie_template` — Rename template (default: `{{title}}{{year}} - {{media_suffix}}`)

## Architecture

```
src/
├── ai/           # AI assist (DeepSeek, Cloudflare, Custom)
├── cli/          # CLI args + command handlers
├── core/         # Scanner, identifier, config, hasher, context inference
├── db/           # Sled-based cache
├── engine/       # Deduplicator, renamer, organizer
├── media/        # Media probe (native MP4/MKV/audio + ffprobe)
├── models/       # Data models (MediaItem, ScrapeResult, etc.)
├── scraper/      # TMDB, MusicBrainz, OpenLibrary, local NFO
└── tui/          # Ratatui terminal UI
```

## License

MIT
