use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Instant, SystemTime};

use crate::junk::{match_junk_category, JunkItem};
use crate::types::{Args, Colors};
use crate::ui::format_number;

#[cfg(unix)]
fn get_file_unique_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn get_file_unique_id(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

pub fn matches_filter(filename: &str, pattern: &str) -> bool {
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    let lower_f = filename.to_lowercase();
    let lower_p = pattern.to_lowercase();
    if let Some(suffix) = lower_p.strip_prefix('*') {
        lower_f.ends_with(suffix)
    } else if let Some(prefix) = lower_p.strip_suffix('*') {
        lower_f.starts_with(prefix)
    } else {
        lower_f.contains(&lower_p)
    }
}

pub fn is_path_excluded(path: &Path, exclude_patterns: &[String]) -> bool {
    if exclude_patterns.is_empty() {
        return false;
    }
    for comp in path.components() {
        let comp_str = comp.as_os_str().to_string_lossy();
        for pattern in exclude_patterns {
            if comp_str.eq_ignore_ascii_case(pattern) {
                return true;
            }
        }
    }
    false
}

pub fn get_path_depth(path: &Path, root: &Path) -> usize {
    path.strip_prefix(root)
        .map(|rel| rel.components().count())
        .unwrap_or_else(|_| {
            path.components()
                .count()
                .saturating_sub(root.components().count())
        })
}

struct WorkPool {
    queue: Mutex<VecDeque<PathBuf>>,
    cv: Condvar,
    active_workers: AtomicUsize,
    shutdown: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct ScanResults {
    pub total_scanned: u64,
    pub total_elapsed: f64,
    pub largest_files: Vec<(u64, PathBuf)>,
    pub largest_folders: Vec<(u64, PathBuf)>,
    pub top_extensions: Vec<(String, u64, u64)>,
    pub age_distribution: Vec<(String, u64, u64)>,
    pub size_histogram: Vec<(String, u64, u64)>,
    pub junk_items: Vec<JunkItem>,
    pub size_candidates: HashMap<u64, Vec<PathBuf>>,
}

fn merge_file_heaps(
    all_heaps: Vec<BinaryHeap<Reverse<(u64, PathBuf)>>>,
    top_n: usize,
) -> Vec<(u64, PathBuf)> {
    let mut combined: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::with_capacity(top_n + 1);
    for heap in all_heaps {
        for item in heap {
            if combined.len() < top_n {
                combined.push(item);
            } else if let Some(min_top) = combined.peek() {
                if item.0 .0 > min_top.0 .0 {
                    combined.pop();
                    combined.push(item);
                }
            }
        }
    }
    let mut vec: Vec<(u64, PathBuf)> = combined.into_iter().map(|Reverse(x)| x).collect();
    vec.sort_by_key(|a| Reverse(a.0));
    vec
}

fn compute_largest_folders(
    dir_sizes: HashMap<PathBuf, u64>,
    drive_roots: &[PathBuf],
    top_n: usize,
    min_size: u64,
) -> Vec<(u64, PathBuf)> {
    let mut recursive_sizes = dir_sizes.clone();
    let mut paths: Vec<PathBuf> = dir_sizes.keys().cloned().collect();

    paths.sort_by_key(|p| Reverse(p.components().count()));

    for p in paths {
        let size = *recursive_sizes.get(&p).unwrap_or(&0);
        if size > 0 {
            if let Some(parent) = p.parent() {
                let is_root = drive_roots.iter().any(|r| r.as_path() == p.as_path());
                if !is_root {
                    if let Some(psize) = recursive_sizes.get_mut(parent) {
                        *psize += size;
                    } else {
                        recursive_sizes.insert(parent.to_path_buf(), size);
                    }
                }
            }
        }
    }

    let mut top_heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::with_capacity(top_n + 1);

    for (path, size) in recursive_sizes {
        if size < min_size {
            continue;
        }

        if drive_roots.iter().any(|r| r.as_path() == path.as_path()) {
            continue;
        }

        if top_heap.len() < top_n {
            top_heap.push(Reverse((size, path)));
        } else if let Some(min) = top_heap.peek() {
            if size > min.0 .0 {
                top_heap.pop();
                top_heap.push(Reverse((size, path)));
            }
        }
    }

    let mut vec: Vec<(u64, PathBuf)> = top_heap.into_iter().map(|Reverse(x)| x).collect();
    vec.sort_by_key(|a| Reverse(a.0));
    vec
}

fn merge_extension_stats(
    all_ext_maps: Vec<HashMap<String, (u64, u64)>>,
    top_n: usize,
) -> Vec<(String, u64, u64)> {
    let mut totals: HashMap<String, (u64, u64)> = HashMap::new();
    for map in all_ext_maps {
        for (ext, (bytes, count)) in map {
            let entry = totals.entry(ext).or_insert((0, 0));
            entry.0 += bytes;
            entry.1 += count;
        }
    }

    let mut sorted: Vec<(String, u64, u64)> = totals
        .into_iter()
        .map(|(ext, (bytes, count))| (ext, bytes, count))
        .collect();

    sorted.sort_by_key(|item| Reverse(item.1));
    if sorted.len() > top_n {
        sorted.truncate(top_n);
    }
    sorted
}

fn merge_age_distributions(all_age_buckets: Vec<[(u64, u64); 5]>) -> Vec<(String, u64, u64)> {
    let mut merged = [(0u64, 0u64); 5];
    for b in all_age_buckets {
        for i in 0..5 {
            merged[i].0 += b[i].0;
            merged[i].1 += b[i].1;
        }
    }

    vec![
        ("< 30 days (recent)".to_string(), merged[0].0, merged[0].1),
        ("30 - 90 days".to_string(), merged[1].0, merged[1].1),
        ("90 - 180 days".to_string(), merged[2].0, merged[2].1),
        ("180 - 365 days".to_string(), merged[3].0, merged[3].1),
        ("> 1 year (dormant)".to_string(), merged[4].0, merged[4].1),
    ]
}

fn merge_size_histograms(all_histograms: Vec<[(u64, u64); 5]>) -> Vec<(String, u64, u64)> {
    let mut merged = [(0u64, 0u64); 5];
    for h in all_histograms {
        for i in 0..5 {
            merged[i].0 += h[i].0;
            merged[i].1 += h[i].1;
        }
    }

    vec![
        ("< 1 KB (tiny)".to_string(), merged[0].0, merged[0].1),
        ("1 KB - 1 MB (small)".to_string(), merged[1].0, merged[1].1),
        (
            "1 MB - 100 MB (medium)".to_string(),
            merged[2].0,
            merged[2].1,
        ),
        (
            "100 MB - 1 GB (large)".to_string(),
            merged[3].0,
            merged[3].1,
        ),
        ("> 1 GB (huge)".to_string(), merged[4].0, merged[4].1),
    ]
}

pub fn run_scan(
    args: &Args,
    valid_drives: &[PathBuf],
    colors: &Colors,
    is_interactive: bool,
) -> ScanResults {
    let scan_start_time = Instant::now();
    let total_files = Arc::new(AtomicU64::new(0));
    let last_scanned_path = Arc::new(Mutex::new(String::new()));
    let seen_inodes = Arc::new(Mutex::new(HashSet::new()));

    let mut all_file_heaps = Vec::new();
    let mut all_dir_sizes: HashMap<PathBuf, u64> = HashMap::new();
    let mut all_ext_maps = Vec::new();
    let mut all_age_buckets = Vec::new();
    let mut all_size_histograms = Vec::new();
    let mut all_junk_items = Vec::new();
    let mut size_candidates: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    for drive_path in valid_drives {
        println!(
            "\n{}--- Starting Scan on Drive {} ---{}",
            colors.yellow,
            drive_path.display(),
            colors.reset
        );

        let pool = Arc::new(WorkPool {
            queue: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            active_workers: AtomicUsize::new(0),
            shutdown: AtomicBool::new(false),
        });

        pool.queue.lock().unwrap().push_back(drive_path.clone());

        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = Arc::clone(&is_running);
        let total_files_clone = Arc::clone(&total_files);
        let last_path_clone = Arc::clone(&last_scanned_path);

        let progress_handle = if is_interactive {
            Some(thread::spawn(move || {
                while is_running_clone.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(50));
                    let count = total_files_clone.load(Ordering::Relaxed);
                    let elapsed = scan_start_time.elapsed().as_secs_f64();
                    let current_path = {
                        let lock = last_path_clone.lock().unwrap();
                        lock.clone()
                    };

                    let truncated = if current_path.chars().count() > 45 {
                        let t: String = current_path.chars().take(45).collect();
                        format!("{}...", t)
                    } else {
                        current_path
                    };

                    print!(
                        "\r\x1b[K\x1b[96m[{:>5.1}s | {:>7} files]\x1b[0m \x1b[2m{}\x1b[0m",
                        elapsed,
                        format_number(count),
                        truncated
                    );
                    std::io::stdout().flush().ok();
                }
                print!("\r\x1b[K");
                std::io::stdout().flush().ok();
            }))
        } else {
            None
        };

        let mut handles = Vec::new();
        for _ in 0..args.threads {
            let pool = Arc::clone(&pool);
            let total_files = Arc::clone(&total_files);
            let last_path = Arc::clone(&last_scanned_path);
            let seen_inodes = Arc::clone(&seen_inodes);
            let top_n = args.top;
            let exclude_patterns = args.exclude.clone();
            let drive_root = drive_path.clone();
            let max_depth = args.max_depth;
            let older_than_days = args.older_than_days;
            let find_duplicates = args.find_duplicates;
            let find_junk = args.find_junk || args.clean_junk;
            let filter_pattern = args.filter_pattern.clone();

            handles.push(thread::spawn(move || {
                let mut local_files: BinaryHeap<Reverse<(u64, PathBuf)>> =
                    BinaryHeap::with_capacity(top_n + 1);
                let mut local_dir_sizes: HashMap<PathBuf, u64> = HashMap::new();
                let mut local_ext_sizes: HashMap<String, (u64, u64)> = HashMap::new();
                let mut local_age: [(u64, u64); 5] = [(0, 0); 5];
                let mut local_hist: [(u64, u64); 5] = [(0, 0); 5];
                let mut local_junk: Vec<JunkItem> = Vec::new();
                let mut local_size_candidates: HashMap<u64, Vec<PathBuf>> = HashMap::new();

                loop {
                    let current_dir = {
                        let mut q = pool.queue.lock().unwrap();
                        loop {
                            if pool.shutdown.load(Ordering::Acquire) {
                                return (
                                    local_files,
                                    local_dir_sizes,
                                    local_ext_sizes,
                                    local_age,
                                    local_hist,
                                    local_junk,
                                    local_size_candidates,
                                );
                            }
                            if let Some(path) = q.pop_front() {
                                pool.active_workers.fetch_add(1, Ordering::SeqCst);
                                break path;
                            }
                            if pool.active_workers.load(Ordering::SeqCst) == 0 {
                                pool.shutdown.store(true, Ordering::Release);
                                pool.cv.notify_all();
                                return (
                                    local_files,
                                    local_dir_sizes,
                                    local_ext_sizes,
                                    local_age,
                                    local_hist,
                                    local_junk,
                                    local_size_candidates,
                                );
                            }
                            q = pool.cv.wait(q).unwrap();
                        }
                    };

                    if is_path_excluded(&current_dir, &exclude_patterns) {
                        let q = pool.queue.lock().unwrap();
                        let prev = pool.active_workers.fetch_sub(1, Ordering::SeqCst);
                        if prev == 1 && q.is_empty() {
                            pool.shutdown.store(true, Ordering::Release);
                            pool.cv.notify_all();
                            return (
                                local_files,
                                local_dir_sizes,
                                local_ext_sizes,
                                local_age,
                                local_hist,
                                local_junk,
                                local_size_candidates,
                            );
                        } else {
                            pool.cv.notify_all();
                            continue;
                        }
                    }

                    if find_junk {
                        if let Some(dir_name) = current_dir.file_name().and_then(|n| n.to_str()) {
                            if let Some(cat) = match_junk_category(dir_name) {
                                local_junk.push(JunkItem {
                                    path: current_dir.clone(),
                                    category: cat,
                                    bytes: 0,
                                });
                            }
                        }
                    }

                    let depth = get_path_depth(&current_dir, &drive_root);
                    let allow_descend = max_depth.map(|m| depth < m).unwrap_or(true);

                    let mut new_subdirs = Vec::new();
                    let mut dir_file_size: u64 = 0;

                    if let Ok(entries) = std::fs::read_dir(&current_dir) {
                        for entry in entries.flatten() {
                            let file_type = match entry.file_type() {
                                Ok(ft) => ft,
                                Err(_) => continue,
                            };

                            if file_type.is_symlink() {
                                continue;
                            }

                            let path = entry.path();
                            if is_path_excluded(&path, &exclude_patterns) {
                                continue;
                            }

                            if file_type.is_dir() {
                                if allow_descend {
                                    new_subdirs.push(path);
                                }
                            } else if file_type.is_file() {
                                let file_name_str =
                                    path.file_name().unwrap_or_default().to_string_lossy();
                                if let Some(ref pattern) = filter_pattern {
                                    if !matches_filter(&file_name_str, pattern) {
                                        continue;
                                    }
                                }

                                let meta = match entry.metadata() {
                                    Ok(m) => m,
                                    Err(_) => continue,
                                };

                                let len = meta.len();
                                if len == 0 {
                                    continue;
                                }

                                if let Some(id) = get_file_unique_id(&meta) {
                                    let mut lock = seen_inodes.lock().unwrap();
                                    if !lock.insert(id) {
                                        continue;
                                    }
                                }

                                dir_file_size += len;
                                total_files.fetch_add(1, Ordering::Relaxed);

                                let hist_idx = if len < 1024 {
                                    0
                                } else if len < 1024 * 1024 {
                                    1
                                } else if len < 100 * 1024 * 1024 {
                                    2
                                } else if len < 1024 * 1024 * 1024 {
                                    3
                                } else {
                                    4
                                };
                                local_hist[hist_idx].0 += len;
                                local_hist[hist_idx].1 += 1;

                                let age_days = meta
                                    .modified()
                                    .ok()
                                    .and_then(|m| SystemTime::now().duration_since(m).ok())
                                    .map(|d| d.as_secs() / 86400)
                                    .unwrap_or(0);

                                let bucket_idx = if age_days < 30 {
                                    0
                                } else if age_days < 90 {
                                    1
                                } else if age_days < 180 {
                                    2
                                } else if age_days < 365 {
                                    3
                                } else {
                                    4
                                };
                                local_age[bucket_idx].0 += len;
                                local_age[bucket_idx].1 += 1;

                                let satisfies_age = older_than_days
                                    .map(|min_d| age_days >= min_d)
                                    .unwrap_or(true);

                                if satisfies_age {
                                    if local_files.len() < top_n {
                                        local_files.push(Reverse((len, path.clone())));
                                    } else if let Some(min_top) = local_files.peek() {
                                        if len > min_top.0 .0 {
                                            local_files.pop();
                                            local_files.push(Reverse((len, path.clone())));
                                        }
                                    }
                                }

                                if find_duplicates && len >= 65536 {
                                    local_size_candidates
                                        .entry(len)
                                        .or_default()
                                        .push(path.clone());
                                }

                                let ext = path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| format!(".{}", e.to_lowercase()))
                                    .unwrap_or_else(|| "(no extension)".to_string());

                                let ext_entry = local_ext_sizes.entry(ext).or_insert((0, 0));
                                ext_entry.0 += len;
                                ext_entry.1 += 1;

                                if let Ok(mut lock) = last_path.try_lock() {
                                    *lock = path.to_string_lossy().to_string();
                                }
                            }
                        }
                    }

                    local_dir_sizes.insert(current_dir, dir_file_size);

                    {
                        let mut q = pool.queue.lock().unwrap();
                        if !new_subdirs.is_empty() {
                            q.extend(new_subdirs);
                        }
                        let prev = pool.active_workers.fetch_sub(1, Ordering::SeqCst);
                        if prev == 1 && q.is_empty() {
                            pool.shutdown.store(true, Ordering::Release);
                            pool.cv.notify_all();
                            return (
                                local_files,
                                local_dir_sizes,
                                local_ext_sizes,
                                local_age,
                                local_hist,
                                local_junk,
                                local_size_candidates,
                            );
                        } else {
                            pool.cv.notify_all();
                        }
                    }
                }
            }));
        }

        for h in handles {
            let (files, dir_sizes, ext_sizes, age_buckets, hist_buckets, junk_entries, size_cands) =
                h.join().unwrap();
            all_file_heaps.push(files);
            for (p, s) in dir_sizes {
                *all_dir_sizes.entry(p).or_insert(0) += s;
            }
            all_ext_maps.push(ext_sizes);
            all_age_buckets.push(age_buckets);
            all_size_histograms.push(hist_buckets);
            all_junk_items.extend(junk_entries);

            for (sz, paths) in size_cands {
                size_candidates.entry(sz).or_default().extend(paths);
            }
        }

        is_running.store(false, Ordering::Relaxed);
        if let Some(h) = progress_handle {
            h.join().unwrap();
        }
    }

    let total_elapsed = scan_start_time.elapsed().as_secs_f64();
    let total_scanned = total_files.load(Ordering::Relaxed);

    println!(
        "{}Scan complete: {} files across {} drive(s) in {:.1}s{}",
        colors.dim,
        format_number(total_scanned),
        valid_drives.len(),
        total_elapsed,
        colors.reset
    );

    let largest_files = merge_file_heaps(all_file_heaps, args.top);
    let largest_folders =
        compute_largest_folders(all_dir_sizes, valid_drives, args.top, args.min_folder_size);
    let top_extensions = merge_extension_stats(all_ext_maps, args.top.min(10));
    let age_distribution = merge_age_distributions(all_age_buckets);
    let size_histogram = merge_size_histograms(all_size_histograms);

    for junk in &mut all_junk_items {
        if let Some(&size) = largest_folders
            .iter()
            .find(|(_, p)| p == &junk.path)
            .map(|(s, _)| s)
        {
            junk.bytes = size;
        }
    }

    ScanResults {
        total_scanned,
        total_elapsed,
        largest_files,
        largest_folders,
        top_extensions,
        age_distribution,
        size_histogram,
        junk_items: all_junk_items,
        size_candidates,
    }
}
