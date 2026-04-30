# Medio CLI Schema

This document defines the machine-readable JSON envelopes returned by `medio --json`.

## Compatibility

- Current schema version: `1`
- Top-level discriminator fields are stable:
  - `schema_version`
  - `kind`
  - `command`
- New fields may be added in future minor updates.
- Existing fields in schema version `1` should be treated as stable.

## Top-Level Kinds

### `pipeline_report`

Returned by commands that primarily describe pipeline discovery or metadata enrichment.

Commands:

- `scan`
- `scrape`

Common fields:

- `schema_version`: string
- `kind`: `"pipeline_report"`
- `command`: `"scan"` or `"scrape"`
- `root`: string
- `item_source`: `"live_scan"` or `"cached_index"`
- `summary`: pipeline summary
- `stages`: array of stage reports
- `items`: array of media items

`scan`-specific fields:

- `processed`: bool
- `scraped`: bool

`scrape`-specific fields:

- `scraped_count`: integer

Example:

```json
{
  "schema_version": "1",
  "kind": "pipeline_report",
  "command": "scan",
  "root": "/media",
  "item_source": "live_scan",
  "summary": {
    "stage_count": 0,
    "item_count": 0,
    "scraped_items": 0
  },
  "processed": true,
  "scraped": false,
  "stages": [],
  "items": []
}
```

### `analysis_report`

Returned by:

- `analyze`

Fields:

- `schema_version`: string
- `kind`: `"analysis_report"`
- `command`: `"analyze"`
- `summary`: analysis summary
- `diagnostics`: array of stage diagnostics
- `stages`: array of stage reports
- `item`: media item
- `duplicate_groups`: array of duplicate groups relevant to the target item
- `rename_plan`: rename plan or `null`
- `organize_plans`: array of organize plans relevant to the target item

Example:

```json
{
  "schema_version": "1",
  "kind": "analysis_report",
  "command": "analyze",
  "summary": {
    "stage_count": 0,
    "duplicate_groups": 0,
    "guarded_duplicate_groups": 0,
    "rename_planned": false,
    "rename_blocked": false,
    "organize_plans": 0,
    "organize_blocked": 0,
    "organize_nfo_ready": 0,
    "organize_image_ready": 0
  },
  "diagnostics": [
    {
      "stage": "identify",
      "decision": "parsed metadata accepted from Regex",
      "evidence": [],
      "risks": []
    }
  ],
  "stages": [],
  "item": {},
  "duplicate_groups": [],
  "rename_plan": null,
  "organize_plans": []
}
```

### `execution_report`

Returned by commands that produce executable plans and execution receipts.

Commands:

- `rename`
- `dedup`
- `organize`

Fields:

- `schema_version`: string
- `kind`: `"execution_report"`
- `command`: `"rename"`, `"dedup"`, or `"organize"`
- `summary`: execution planning summary
- `entries`: array of command-specific plan/group entries
- `report`: execution report
- `dry_run`: bool
- `aborted`: bool

Example:

```json
{
  "schema_version": "1",
  "kind": "execution_report",
  "command": "rename",
  "summary": {
    "entry_count": 0,
    "ready_entries": 0,
    "blocked_entries": 0,
    "guarded_entries": 0
  },
  "entries": [],
  "report": {
    "operation": "rename",
    "executed": 0,
    "blocked": 0,
    "guarded": 0,
    "skipped": 0,
    "errors": 0,
    "asset_generated": 0,
    "details": []
  },
  "dry_run": true,
  "aborted": false
}
```

## Stage Report

Each pipeline stage entry contains:

- `stage`: string
- `item_count`: integer
- `details`: string array

## Pipeline Summary

The nested `summary` object in `pipeline_report` contains:

- `stage_count`: integer
- `item_count`: integer
- `scraped_items`: integer

## Analysis Summary

The nested `summary` object in `analysis_report` contains:

- `stage_count`: integer
- `duplicate_groups`: integer
- `guarded_duplicate_groups`: integer
- `rename_planned`: bool
- `rename_blocked`: bool
- `organize_plans`: integer
- `organize_blocked`: integer
- `organize_nfo_ready`: integer
- `organize_image_ready`: integer

## Analysis Diagnostics

Each `diagnostics` entry contains:

- `stage`: string
- `decision`: string
- `evidence`: string array
- `risks`: string array

## Execution Planning Summary

The nested `summary` object in `execution_report` contains:

- `entry_count`: integer
- `ready_entries`: integer
- `blocked_entries`: integer
- `guarded_entries`: integer

## Execution Report

The nested `report` object contains:

- `operation`: string
- `executed`: integer
- `blocked`: integer
- `guarded`: integer
- `skipped`: integer
- `errors`: integer
- `asset_generated`: integer
- `details`: string array

`details` is intended for human-readable audit lines. Consumers should prefer the structured counters for automation.

## Notes for Integrators

- `entries` is intentionally generic across execution commands:
  - `rename` returns rename plans
  - `dedup` returns duplicate groups
  - `organize` returns organize plans
- Entry payloads may include a `decision` object with machine-readable planning signals in addition
  to human-readable `rationale` text. Prefer `decision` for automation when present.
- `details` may grow and should not be parsed as a strict protocol.
- For compatibility checks, gate on `schema_version`, then branch on `kind` and `command`.

## Media Item Evidence Fields

`pipeline_report.items[]` and `analysis_report.item` serialize the internal `MediaItem` shape. In
schema version `1`, integrators should expect these additional scraping-depth fields:

- `content_evidence`: structured, file-content-derived signals used during scraping and identity
  confirmation
- `identity_resolution`: candidate ranking and confirmation result for the selected metadata target

### `content_evidence`

Fields:

- `container`: container-level metadata probe results
- `subtitles`: subtitle-derived evidence entries
- `visual`: OCR / frame-text evidence entries
- `audio`: ASR / transcript evidence entries
- `title_candidates`: deduplicated title hypotheses extracted from content
- `season_hypotheses`: season numbers inferred from content
- `episode_hypotheses`: episode numbers inferred from content
- `runtime_secs`: measured runtime when available
- `risk_flags`: probe gaps or warnings

#### `container`

Fields:

- `format_name`: container format label such as `matroska,webm`
- `title`: embedded container title if present
- `comment`: embedded container comment if present
- `chapters`: chapter titles
- `stream_languages`: detected stream language tags
- `track_titles`: detected stream track titles

#### `subtitles[]`

Fields:

- `source`: `"ExternalText"` or `"EmbeddedTrack"`
- `locator`: subtitle file path or embedded stream locator
- `language`: stream or file language when known
- `track_title`: embedded subtitle track title when known
- `sample_lines`: short extracted dialogue / text samples
- `title_candidates`: title hypotheses derived from subtitle content
- `season`: season hint inferred from subtitle content
- `episode`: episode hint inferred from subtitle content

### `identity_resolution`

Fields:

- `confirmation_state`: final confirmation status
- `best`: best-ranked candidate or `null`
- `candidates`: ranked candidate list
- `evidence_refs`: compact evidence summary strings
- `risk_flags`: ambiguity or confirmation warnings

#### `confirmation_state`

Possible values:

- `Confirmed`: multiple strong content sources corroborate the selected identity
- `HighConfidenceCandidate`: best candidate selected, but content corroboration is not yet strong
  enough to hard-confirm
- `AmbiguousCandidates`: top candidates are too close to safely confirm
- `InsufficientEvidence`: provider metadata exists, but content-derived evidence is too weak to
  trust identity

#### `identity_resolution.best` / `identity_resolution.candidates[]`

Fields:

- `source`: scrape provider used for the candidate
- `title`: candidate title
- `year`: candidate year
- `season`: candidate season when applicable
- `episode`: candidate episode when applicable
- `episode_title`: candidate episode title when applicable
- `score`: internal ranking score used for candidate ordering
- `evidence`: scoring rationale excerpts
