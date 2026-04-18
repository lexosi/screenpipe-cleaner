# screenpipe-manager

A CLI tool to manage [screenpipe](https://github.com/mediar-ai/screenpipe) recordings: clean up old data, enforce retention policies, and inspect disk usage and database statistics.

Built as a portfolio project — the emphasis is on clean module boundaries, idiomatic Rust, and real-world utility.

---

## Features

| Command | Description |
|---------|-------------|
| `cleanup` | Delete DB records and physical files older than the configured retention window |
| `status`  | Display current config, disk usage, and per-table record counts |

**Cleanup highlights:**
- Deletes rows from `frames`, `audio_chunks`, `audio_transcriptions`, `ocr_text`, and `video_chunks`
- Also removes the referenced `.jpg` / `.mp4` files from disk
- Respects a **blacklist**: purges records for apps you never want stored
- `--dry-run` flag previews every deletion without touching anything
- `--days` flag overrides the configured retention period for a one-off run

---

## Installation

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable, 1.75+)

### Build

```bash
git clone https://github.com/your-username/screenpipe-manager
cd screenpipe-manager
cargo build --release
```

The binary is written to `target/release/screenpipe-manager`.

---

## Configuration

Copy the example config to the same directory as the binary:

```bash
cp config.toml.example config.toml
```

Edit `config.toml` to match your setup. All fields have sensible defaults, so the tool works without a config file.

```toml
retention_days = 7        # Keep 7 days of recordings
max_storage_gb = 10.0     # Informational cap (GB)
data_dir = ""             # Empty = ~/.screenpipe

record_audio         = true
record_screen        = true
record_transcription = true

blacklist = ["1Password", "KeePass"]   # Always purge these apps
whitelist = []                         # Empty = record all apps
```

### Config search order

The binary looks for `config.toml` in the same directory as the executable.  
If no file is found, built-in defaults are used.

---

## Usage

### `cleanup`

Remove records and files older than `retention_days` (default: 7).

```bash
# Delete records older than 7 days (from config)
screenpipe-manager cleanup

# Preview without deleting
screenpipe-manager cleanup --dry-run

# Override retention to 30 days for this run
screenpipe-manager cleanup --days 30

# Combine: preview a 30-day cleanup
screenpipe-manager cleanup --dry-run --days 30
```

**Example output:**

```
Retention: 7 days  |  Cutoff: 2026-04-11T12:00:00+00:00

=== Cleanup Summary ===
  Deleted 142    frame records
  Deleted 38     audio_chunk records
  Deleted 38     audio_transcription records
  Deleted 142    ocr_text records
  Deleted 14     video_chunk records
  Deleted 180    physical files
  4 files were already absent on disk
```

### `status`

Print configuration, disk usage, and DB table sizes.

```bash
screenpipe-manager status
```

**Example output:**

```
=== Configuration ===
  retention_days        : 7
  max_storage_gb        : 10.0
  data_dir              : /home/user/.screenpipe
  record_audio          : true
  record_screen         : true
  record_transcription  : true
  blacklist             : 1Password, KeePass
  whitelist             : (none — record all apps)

=== Disk Usage ===
  2.34 GB  (/home/user/.screenpipe)

=== Database Record Counts ===
  DB path: /home/user/.screenpipe/db.sqlite
  frames                  : 18 432 records
  audio_chunks            : 4 801 records
  audio_transcriptions    : 4 801 records
  ocr_text                : 18 432 records
  video_chunks            : 1 204 records
```

---

## Project Structure

```
screenpipe-manager/
├── src/
│   ├── main.rs      — CLI entry point (clap subcommands) + inline status module
│   ├── config.rs    — Config struct, TOML loading, path resolution
│   ├── cleanup.rs   — Deletion logic: DB records + physical files
│   ├── filter.rs    — Blacklist / whitelist predicate logic (with unit tests)
│   └── storage.rs   — Recursive disk-usage calculation, file deletion helper
├── config.toml.example
├── Cargo.toml
└── README.md
```

---

## Database Schema Assumptions

screenpipe-manager assumes the following screenpipe schema:

| Table | Key columns used |
|-------|-----------------|
| `frames` | `id`, `timestamp` (ISO 8601), `file_path`, `app_name` |
| `audio_chunks` | `id`, `timestamp`, `file_path` |
| `audio_transcriptions` | `audio_chunk_id` (FK → audio_chunks) |
| `ocr_text` | `frame_id` (FK → frames) |
| `video_chunks` | `frame_id` (FK → frames) |

Timestamps are stored as ISO 8601 strings with timezone offset, e.g.:  
`2026-04-18T15:30:28.444141700+00:00`

---

## License

MIT
