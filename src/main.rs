use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Instant;

const COLOR_CYAN: &str = "\x1b[96m";
const COLOR_GREEN: &str = "\x1b[92m";
const COLOR_YELLOW: &str = "\x1b[93m";
const COLOR_RED: &str = "\x1b[91m";
const COLOR_RESET: &str = "\x1b[0m";
const COLOR_DIM: &str = "\x1b[2m";

const DEFAULT_RESULTS_TO_SHOW: usize = 20;
const MINIMUM_FOLDER_SIZE_BYTES: u64 = 1024 * 1024;
const SEPARATOR_WIDTH: usize = 80;

#[cfg(windows)]
fn enable_ansi_support() -> bool {
    use std::os::raw::c_void;
    type Handle = *mut c_void;
    type Dword = u32;
    type Bool = i32;

    const STD_OUTPUT_HANDLE: Dword = -11i32 as Dword;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: Dword = 0x0004;

    extern "system" {
        fn GetStdHandle(nStdHandle: Dword) -> Handle;
        fn GetConsoleMode(hConsoleHandle: Handle, lpMode: *mut Dword) -> Bool;
        fn SetConsoleMode(hConsoleHandle: Handle, dwMode: Dword) -> Bool;
    }

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle as isize == -1 {
            return false;
        }
        let mut mode: Dword = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        } else {
            false
        }
    }
}

#[cfg(not(windows))]
fn enable_ansi_support() -> bool {
    true
}

fn format_number(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut s = String::new();
    let mut count = 0;
    while n > 0 {
        if count > 0 && count % 3 == 0 {
            s.push(',');
        }
        s.push((b'0' + (n % 10) as u8) as char);
        n /= 10;
        count += 1;
    }
    s.chars().rev().collect()
}

fn bytes_to_readable_size(byte_count: u64) -> String {
    let size_in_gb = byte_count as f64 / (1024.0 * 1024.0 * 1024.0);
    if size_in_gb >= 0.01 {
        format!("{:.2} GB", size_in_gb)
    } else {
        let size_in_mb = byte_count as f64 / (1024.0 * 1024.0);
        if size_in_mb >= 0.05 {
            format!("{:.1} MB", size_in_mb)
        } else {
            let size_in_kb = byte_count as f64 / 1024.0;
            if size_in_kb >= 0.1 {
                format!("{:.1} KB", size_in_kb)
            } else {
                format!("{} B", byte_count)
            }
        }
    }
}

fn print_results_table(table_title: &str, rows: &[(u64, PathBuf)]) {
    let separator = "=".repeat(SEPARATOR_WIDTH);
    println!("\n{}{}{}", COLOR_YELLOW, separator, COLOR_RESET);
    println!(
        "{}{: <12}{} | {}{}{}",
        COLOR_GREEN, "SIZE", COLOR_RESET, COLOR_YELLOW, table_title, COLOR_RESET
    );
    println!("{}{}{}", COLOR_YELLOW, separator, COLOR_RESET);
    if rows.is_empty() {
        println!("  (No items found)");
    } else {
        for (bytes, path) in rows {
            let size_str = bytes_to_readable_size(*bytes);
            println!(
                "{}{: <12}{} | {}",
                COLOR_GREEN,
                size_str,
                COLOR_RESET,
                path.display()
            );
        }
    }
}

struct WorkPool {
    queue: Mutex<VecDeque<PathBuf>>,
    cv: Condvar,
    active_workers: AtomicUsize,
    shutdown: AtomicBool,
}

struct Args {
    drives: Vec<String>,
    top: usize,
    quiet: bool,
    threads: usize,
    min_folder_size: u64,
    pause: bool,
    no_pause: bool,
}

fn pause_before_exit(args: Option<&Args>) {
    let should_pause = match args {
        Some(a) if a.no_pause => false,
        Some(a) if a.pause => true,
        _ => {
            #[cfg(windows)]
            {
                std::io::stdin().is_terminal()
            }
            #[cfg(not(windows))]
            {
                args.map(|a| a.pause).unwrap_or(false)
            }
        }
    };

    if should_pause {
        println!("\nPress Enter to exit...");
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
    }
}

fn print_help() {
    println!("Disk Usage Scanner (Rust)\n");
    println!("Usage:");
    println!("  disk-usage-scanner [OPTIONS]\n");
    println!("Options:");
    println!("  --drives <PATH...>   Drives or paths to scan");
    println!("  --top <N>            Number of results to show (default: 20)");
    println!("  --quiet              Suppress live progress output");
    println!("  --threads <N>        Number of worker threads (default: CPU cores)");
    println!("  --min-size <BYTES>   Minimum folder size in bytes (default: 1048576)");
    println!("  --pause              Prompt for Enter before closing");
    println!("  --no-pause           Do not prompt for Enter before closing");
    println!("  -h, --help           Print help information");
}

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut drives = Vec::new();
    let mut top = DEFAULT_RESULTS_TO_SHOW;
    let mut quiet = false;
    let mut threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let mut min_folder_size = MINIMUM_FOLDER_SIZE_BYTES;
    let mut pause = false;
    let mut no_pause = false;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--drives" => {
                i += 1;
                while i < raw.len() && !raw[i].starts_with("--") {
                    drives.push(raw[i].clone());
                    i += 1;
                }
                continue;
            }
            "--top" => {
                i += 1;
                if i < raw.len() {
                    top = raw[i]
                        .parse()
                        .map_err(|_| format!("Invalid value for --top: {}", raw[i]))?;
                } else {
                    return Err("Missing argument for --top".to_string());
                }
            }
            "--quiet" => {
                quiet = true;
            }
            "--threads" => {
                i += 1;
                if i < raw.len() {
                    threads = raw[i]
                        .parse()
                        .map_err(|_| format!("Invalid value for --threads: {}", raw[i]))?;
                } else {
                    return Err("Missing argument for --threads".to_string());
                }
            }
            "--min-size" => {
                i += 1;
                if i < raw.len() {
                    min_folder_size = raw[i]
                        .parse()
                        .map_err(|_| format!("Invalid value for --min-size: {}", raw[i]))?;
                } else {
                    return Err("Missing argument for --min-size".to_string());
                }
            }
            "--pause" => {
                pause = true;
            }
            "--no-pause" => {
                no_pause = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                return Err(format!("Unknown option: {}", other));
            }
        }
        i += 1;
    }

    if drives.is_empty() {
        if cfg!(windows) {
            drives = vec!["C:\\".to_string(), "D:\\".to_string()];
        } else {
            drives = vec!["/".to_string()];
        }
    }

    Ok(Args {
        drives,
        top,
        quiet,
        threads: threads.max(1),
        min_folder_size,
        pause,
        no_pause,
    })
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

fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("\nUnexpected error occurred: {}", info);
        pause_before_exit(None);
    }));

    let ansi_supported = enable_ansi_support();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            pause_before_exit(None);
            std::process::exit(1);
        }
    };

    let is_interactive = std::io::stdout().is_terminal() && !args.quiet && ansi_supported;
    let mut valid_drives = Vec::new();

    for drive in &args.drives {
        let p = PathBuf::from(drive);
        if !p.exists() {
            println!(
                "{}Drive {:?} not found (skipping).{}",
                COLOR_RED, drive, COLOR_RESET
            );
            continue;
        }
        valid_drives.push(p);
    }

    if valid_drives.is_empty() {
        println!("{}No valid drives to scan.{}", COLOR_RED, COLOR_RESET);
        pause_before_exit(Some(&args));
        return;
    }

    let scan_start_time = Instant::now();
    let total_files = Arc::new(AtomicU64::new(0));
    let last_scanned_path = Arc::new(Mutex::new(String::new()));

    let mut all_file_heaps = Vec::new();
    let mut all_dir_sizes: HashMap<PathBuf, u64> = HashMap::new();

    for drive_path in &valid_drives {
        println!(
            "\n{}--- Starting Scan on Drive {} ---{}",
            COLOR_YELLOW,
            drive_path.display(),
            COLOR_RESET
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
                        "\r\x1b[K{}[{:>5.1}s | {:>7} files]{} {}{}{}",
                        COLOR_CYAN,
                        elapsed,
                        format_number(count),
                        COLOR_RESET,
                        COLOR_DIM,
                        truncated,
                        COLOR_RESET
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
            let top_n = args.top;

            handles.push(thread::spawn(move || {
                let mut local_files: BinaryHeap<Reverse<(u64, PathBuf)>> =
                    BinaryHeap::with_capacity(top_n + 1);
                let mut local_dir_sizes: HashMap<PathBuf, u64> = HashMap::new();

                loop {
                    let current_dir = {
                        let mut q = pool.queue.lock().unwrap();
                        loop {
                            if pool.shutdown.load(Ordering::Acquire) {
                                return (local_files, local_dir_sizes);
                            }
                            if let Some(path) = q.pop_front() {
                                pool.active_workers.fetch_add(1, Ordering::SeqCst);
                                break path;
                            }
                            if pool.active_workers.load(Ordering::SeqCst) == 0 {
                                pool.shutdown.store(true, Ordering::Release);
                                pool.cv.notify_all();
                                return (local_files, local_dir_sizes);
                            }
                            q = pool.cv.wait(q).unwrap();
                        }
                    };

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
                            if file_type.is_dir() {
                                new_subdirs.push(path);
                            } else if file_type.is_file() {
                                let len = match entry.metadata() {
                                    Ok(m) => m.len(),
                                    Err(_) => continue,
                                };

                                if len == 0 {
                                    continue;
                                }

                                dir_file_size += len;
                                total_files.fetch_add(1, Ordering::Relaxed);

                                if local_files.len() < top_n {
                                    local_files.push(Reverse((len, path.clone())));
                                } else if let Some(min_top) = local_files.peek() {
                                    if len > min_top.0 .0 {
                                        local_files.pop();
                                        local_files.push(Reverse((len, path.clone())));
                                    }
                                }

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
                            return (local_files, local_dir_sizes);
                        } else {
                            pool.cv.notify_all();
                        }
                    }
                }
            }));
        }

        for h in handles {
            let (files, dir_sizes) = h.join().unwrap();
            all_file_heaps.push(files);
            for (p, s) in dir_sizes {
                *all_dir_sizes.entry(p).or_insert(0) += s;
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
        COLOR_DIM,
        format_number(total_scanned),
        valid_drives.len(),
        total_elapsed,
        COLOR_RESET
    );

    let largest_files = merge_file_heaps(all_file_heaps, args.top);
    let largest_folders =
        compute_largest_folders(all_dir_sizes, &valid_drives, args.top, args.min_folder_size);

    print_results_table(&format!("TOP {} LARGEST FILES", args.top), &largest_files);
    print_results_table(
        &format!("TOP {} LARGEST FOLDERS", args.top),
        &largest_folders,
    );
    println!();

    pause_before_exit(Some(&args));
}
