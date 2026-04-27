<div align="center">

# 🎬 Medio

**Media file manager: rename, deduplicate, organize.**

[![CI](https://github.com/3kaiu/Medio/actions/workflows/ci.yml/badge.svg)](https://github.com/3kaiu/Medio/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![macOS](https://img.shields.io/badge/macOS-arm64%20%7C%20x86_64-blue)](https://github.com/3kaiu/Medio/releases)
[![Linux](https://img.shields.io/badge/Linux-x86_64-orange)](https://github.com/3kaiu/Medio/releases)

All-in-one media toolkit: FileBot + TinyMediaManager + dupeGuru in a single binary.

</div>

---

## ✨ Features

- **Scan** — Recursive media file discovery with smart identification (movie/TV/music/book/STRM)
- **Dedup** — Progressive xxHash deduplication with quality-aware keep strategy
- **Scrape** — Metadata from local NFO, TMDB, MusicBrainz, OpenLibrary, AI, and parsed fallback
- **Rename** — Template-based renaming with subtitle file tracking
- **Analyze** — Single-file deep analysis (hash, quality probe, scrape, AI)
- **Organize** — Archive/local/rename modes with NFO generation and image download
- **TUI** — Interactive terminal UI with search, plan previews, and in-app confirm/execute for dedup/rename/organize

## 🚀 Quick Start

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

## 📖 Usage

```
me                          # Interactive TUI
me scan /path/to/media      # Scan and identify media files
me dedup /path/to/media     # Deduplicate files (dry-run by default)
me scrape /path/to/media    # Scrape metadata only
me rename /path/to/media    # Rename files (dry-run by default)
me analyze /path/to/file    # Analyze a single file
me organize /path/to/media  # Organize into library structure
me config                   # Show config status
me config --init            # Interactive config wizard
me --version                # Show version
me --help                   # Show help
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

TUI notes:

- `Scan` tab shows scanned items and scraped metadata previews
- `Dedup`, `Rename`, and `Organize` tabs show real engine-generated previews
- Press `x` in `Dedup`, `Rename`, or `Organize` to enter confirm mode and execute the current tab's plans
- In dry-run mode, TUI execution stays preview-only; with `dry_run=false`, successful actions trigger an automatic rescan

## ⚙️ Configuration

```
me config          # Show config status
me config --init   # Interactive wizard (TMDB key, AI, organize root)
```

Config file: `~/.config/medio/config.toml`

Key settings:

- `api.tmdb_key` — TMDB API key for movie/TV metadata
- `api.musicbrainz_user_agent` — MusicBrainz user agent
- `ai.deepseek.key` — DeepSeek API key for AI-assisted identification
- `scrape.fallback_chain` — Ordered scrape chain, e.g. `["local", "tmdb", "musicbrainz", "openlibrary", "ai", "guess"]`
- `general.operation_log` — Enable or disable operation logging
- `rename.movie_template` — Rename template (default: `{{title}}{{year}} - {{media_suffix}}`)
- `cache.ttl_days` — TTL for cache cleanup before scrape/hash reuse

## 🏗 Architecture

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

## 🎯 Organize Modes

| Mode      | Behavior                                                                    |
| --------- | --------------------------------------------------------------------------- |
| `archive` | Move to organized library tree (`Movies/`, `TV Shows/`, `Music/`, `Books/`) |
| `local`   | Reorganize in-place within current directory                                |
| `rename`  | Rename only, keep same directory                                            |

## 🔗 Link Modes

| Mode           | Flag          | Behavior                  |
| -------------- | ------------- | ------------------------- |
| Copy (default) | `--link none` | Copy files to target      |
| Hard link      | `--link hard` | Hard link (same disk)     |
| Symlink        | `--link sym`  | Symbolic link to original |

## 🧠 AI Integration

Medio supports AI-assisted identification for ambiguous filenames:

- **DeepSeek** — Default provider, fast and affordable
- **Cloudflare Workers AI** — Edge inference
- **Custom** — Any OpenAI-compatible API

```bash
me config --init          # Set up AI provider interactively
me --no-ai scan /path     # Disable AI for this run
```

Rename notes:

- `rename` and `organize --mode rename` both scrape metadata before planning names
- `preserve_media_suffix=true` appends the suffix when the template omits `media_suffix`
- `season_offset` applies to parsed and scraped season numbers
- `rename_subtitles=false` disables subtitle companion renames

## 📊 Performance

- Pre-compiled regex patterns (3-5x faster on scan/identify/rename)
- Concurrent scraping with `buffer_unordered` (N× speedup, default concurrency=3)
- Reused HTTP connections (connection pooling for image downloads)
- Rayon parallel hashing and identification
- 3.5MB single binary, zero runtime dependencies

## 📄 License

MIT
