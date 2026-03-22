# Disk Usage Scanner

Scans drives and reports the top N largest files and folders.

## Requirements

Python 3.10+, no external dependencies.

## Usage

```
python scanner.py [--drives DRIVE ...] [--top N] [--quiet]
```

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--drives` | `C:\ D:\` | One or more drives to scan |
| `--top N` | `20` | Number of results to show |
| `--quiet` | off | Suppress live progress output |

## Examples

```bash
# Scan default drives, show top 20
python scanner.py

# Scan a single drive, show top 10
python scanner.py --drives C:\ --top 10

# Pipe output to a file
python scanner.py --quiet > results.txt
```

## Notes

- Symlinks and empty files are skipped
- Folders under 1 MB are excluded from the folders list
- Requires read access to the scanned directories; unreadable paths are silently skipped
