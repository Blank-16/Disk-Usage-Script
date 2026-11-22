use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct DiffItem {
    pub path: PathBuf,
    pub old_size: u64,
    pub new_size: u64,
    pub delta: i64,
}

pub fn save_snapshot(
    file_path: &str,
    largest_files: &[(u64, PathBuf)],
    largest_folders: &[(u64, PathBuf)],
) -> std::io::Result<()> {
    let mut file = File::create(file_path)?;
    writeln!(file, "# DISK_USAGE_SNAPSHOT v1")?;
    writeln!(file, "[FOLDERS]")?;
    for (size, path) in largest_folders {
        writeln!(file, "{}\t{}", size, path.display())?;
    }
    writeln!(file, "[FILES]")?;
    for (size, path) in largest_files {
        writeln!(file, "{}\t{}", size, path.display())?;
    }
    Ok(())
}

pub fn load_snapshot(file_path: &str) -> std::io::Result<HashMap<PathBuf, u64>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut map = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }
        if let Some((size_str, path_str)) = trimmed.split_once('\t') {
            if let Ok(size) = size_str.parse::<u64>() {
                map.insert(PathBuf::from(path_str), size);
            }
        }
    }

    Ok(map)
}

pub fn compute_diff(
    snapshot: &HashMap<PathBuf, u64>,
    current_folders: &[(u64, PathBuf)],
    current_files: &[(u64, PathBuf)],
) -> Vec<DiffItem> {
    let mut diffs = Vec::new();

    for (new_size, path) in current_folders.iter().chain(current_files.iter()) {
        let old_size = snapshot.get(path).copied().unwrap_or(0);
        let delta = *new_size as i64 - old_size as i64;
        if delta != 0 {
            diffs.push(DiffItem {
                path: path.clone(),
                old_size,
                new_size: *new_size,
                delta,
            });
        }
    }

    diffs.sort_by_key(|d| std::cmp::Reverse(d.delta));
    diffs
}
