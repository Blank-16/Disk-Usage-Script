# Disk Usage Scanner

<p align="center">
  <img src="assets/cat.png" alt="Disk Usage Scanner Cat Logo" width="128" height="128" />
</p>

Fast multi-threaded disk usage scanner for Unix (Linux, macOS) and Windows. Traverses drives concurrently to find the largest files and folders.

Written in Rust for optimal performance with zero external dependencies, alongside the original Python version.

## Download Pre-built Executables

Pre-compiled standalone executables for Unix and Windows are built on every commit and release via GitHub Actions CI:

1. Go to the Actions tab on GitHub.
2. Select the latest workflow run.
3. Download the executable under Artifacts:
   - Linux: `disk-usage-scanner-linux-x86_64` (standalone static musl binary)
   - Windows: `disk-usage-scanner-windows-x86_64.exe`
   - macOS: `disk-usage-scanner-macos-universal` (Intel and Apple Silicon)

Tagged releases (e.g. `v1.0.0`) automatically publish these binaries under the GitHub Releases page.

## Building from Source

### Prerequisites
- Rust 1.70 or newer (uses standard library only)

```bash
cargo build --release
```

The compiled binary will be placed at `target/release/disk-usage-scanner` (`disk-usage-scanner.exe` on Windows).

## Usage

```bash
# Run with defaults
./target/release/disk-usage-scanner

# Scan specific drives or paths
./target/release/disk-usage-scanner --drives /home /var

# Windows example
disk-usage-scanner.exe --drives C:\ D:\ --top 10

# Quiet mode (suppresses live progress animation)
./target/release/disk-usage-scanner --quiet > report.txt
```

### CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--drives PATH [PATH ...]` | `C:\ D:\` (Windows), `/` (Unix) | Drives or paths to scan |
| `--top N` | `20` | Number of results to show |
| `--quiet` | off | Suppress live progress output |
| `--threads N` | CPU count | Worker thread pool size |
| `--min-size BYTES` | `1048576` (1 MB) | Minimum folder size threshold in bytes |
| `--exclude PATTERN [PATTERN ...]` | none | Folder names or patterns to skip (e.g. node_modules target) |
| `--max-depth N` | none | Maximum folder depth to scan relative to drive root |
| `--older-than DAYS` | none | Filter files older than N days in largest files list |
| `--find-duplicates` | off | Detect duplicate files and report wasted space |
| `--interactive-clean` | off | Interactively select and delete large files |
| `--json FILE` | none | Export scan results to a JSON file |
| `--csv FILE` | none | Export scan results to a CSV file |
| `--html FILE` | none | Export an interactive HTML treemap report |
| `--no-color` | off | Disable ANSI color output |
| `--pause` | on (Windows GUI) | Prompt for Enter before exiting |
| `--no-pause` | off | Exit immediately without prompt |
| `-h, --help` | | Show help information |

## Python Fallback

The original Python script is retained for environments without a Rust compiler:

```bash
python3 script.py [--drives DRIVE ...] [--top N] [--quiet]
```

## Features

- Parallel work-stealing traversal utilizing all CPU cores.
- Drive capacity and used space summary bar.
- Visual horizontal proportion bars showing space distribution at a glance.
- Aggregated recursive folder size calculations.
- File extension and type usage breakdown.
- File age and dormancy distribution analysis (`--older-than`).
- Fast duplicate file detection with two-tier checksums (`--find-duplicates`).
- Interactive file cleanup mode (`--interactive-clean`).
- Interactive offline HTML visual treemap report (`--html`).
- Hard link deduplication (avoids counting duplicate inodes/file IDs).
- Pattern and directory exclusion (`--exclude`).
- Traversal depth limiter (`--max-depth`).
- Export to structured JSON and CSV formats.
- Zero runtime external dependencies.
- Handles inaccessible directories and permission errors gracefully.
- Skips symlinks to prevent duplicate counts and traversal loops.
- Cross-platform support for Windows, Linux, and macOS.
