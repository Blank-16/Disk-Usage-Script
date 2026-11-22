use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct JunkItem {
    pub path: PathBuf,
    pub category: &'static str,
    pub bytes: u64,
}

pub fn match_junk_category(dir_name: &str) -> Option<&'static str> {
    let lower = dir_name.to_lowercase();
    match lower.as_str() {
        "node_modules" => Some("Node.js Dependencies"),
        "target" => Some("Rust/Build Artifacts"),
        ".cache" | "cache" => Some("System/App Cache"),
        "__pycache__" | ".pytest_cache" | ".mypy_cache" | ".tox" => Some("Python Cache"),
        ".gradle" | ".m2" => Some("JVM/Gradle Cache"),
        ".venv" | "venv" => Some("Python Virtualenv"),
        ".next" | ".nuxt" | ".turbo" => Some("Web Framework Build Cache"),
        "tmp" | "temp" => Some("Temporary Directory"),
        ".ds_store" | "thumbs.db" => Some("OS Metadata Junk"),
        _ => None,
    }
}

pub fn clean_junk_items(items: &[JunkItem]) -> (usize, u64) {
    let mut deleted_count = 0;
    let mut reclaimed_bytes = 0;

    for item in items {
        if item.path.is_dir() {
            if std::fs::remove_dir_all(&item.path).is_ok() {
                deleted_count += 1;
                reclaimed_bytes += item.bytes;
            }
        } else if item.path.is_file() && std::fs::remove_file(&item.path).is_ok() {
            deleted_count += 1;
            reclaimed_bytes += item.bytes;
        }
    }

    (deleted_count, reclaimed_bytes)
}

#[allow(dead_code)]
pub fn calculate_junk_totals(items: &[JunkItem]) -> u64 {
    items.iter().map(|item| item.bytes).sum()
}
