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

## Install

```bash
cargo build --release
# Binary at ./target/release/medio
```

## Usage

```bash
# Scan media files
medio scan /path/to/media

# Deduplicate (dry-run by default)
medio dedup /path/to/media
medio --dry-run=false dedup /path/to/media  # execute

# Scrape metadata
medio scrape /path/to/media

# Rename files
medio rename /path/to/media

# Analyze a single file
medio analyze /path/to/file.mp4

# Organize into library structure
medio organize /path/to/media --mode archive
medio organize /path/to/media --mode local --with-nfo --with-images

# Interactive TUI
medio tui

# JSON output for any command
medio --json scan /path/to/media
```

## Configuration

Config file: `~/Library/Application Support/medio/config.toml` (macOS) or `~/.config/medio/config.toml` (Linux)

```bash
medio config  # Show config path
```

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
