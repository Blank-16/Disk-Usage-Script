use crate::types::{Args, DEFAULT_RESULTS_TO_SHOW, MINIMUM_FOLDER_SIZE_BYTES};

#[cfg(windows)]
pub fn enable_ansi_support() -> bool {
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
pub fn enable_ansi_support() -> bool {
    true
}

pub fn print_help() {
    println!("Disk Usage Scanner (Rust)\n");
    println!("Usage:");
    println!("  disk-usage-scanner [OPTIONS]\n");
    println!("Options:");
    println!("  --drives <PATH...>       Drives or paths to scan (default: C:\\ D:\\ on Windows, / on Unix)");
    println!("  --top <N>                Number of results to show in tables (default: 20)");
    println!("  --quiet                  Suppress live progress output");
    println!("  --threads <N>            Number of worker threads (default: CPU cores)");
    println!(
        "  --min-size <BYTES>       Minimum folder size threshold in bytes (default: 1048576)"
    );
    println!(
        "  --exclude <PATTERN...>   Folder names or patterns to skip (e.g. node_modules target)"
    );
    println!("  --max-depth <N>          Maximum folder depth to scan relative to drive root");
    println!("  --older-than <DAYS>      Filter files older than N days in largest files list");
    println!("  --find-duplicates        Detect duplicate files and report wasted space");
    println!("  --interactive-clean      Interactively select and delete large files");
    println!("  --json <FILE>            Export scan results to a JSON file");
    println!("  --csv <FILE>             Export scan results to a CSV file");
    println!("  --html <FILE>            Export an interactive HTML treemap report");
    println!("  --no-color               Disable ANSI color codes");
    println!("  --pause                  Prompt for Enter before closing");
    println!("  --no-pause               Do not prompt for Enter before closing");
    println!("  -h, --help               Print help information");
}

pub fn parse_args() -> Result<Args, String> {
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
    let mut exclude = Vec::new();
    let mut json_out = None;
    let mut csv_out = None;
    let mut html_out = None;
    let mut older_than_days = None;
    let mut find_duplicates = false;
    let mut max_depth = None;
    let mut interactive_clean = false;
    let mut no_color = std::env::var("NO_COLOR").is_ok();

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
            "--exclude" => {
                i += 1;
                while i < raw.len() && !raw[i].starts_with("--") {
                    exclude.push(raw[i].clone());
                    i += 1;
                }
                continue;
            }
            "--max-depth" => {
                i += 1;
                if i < raw.len() {
                    max_depth = Some(
                        raw[i]
                            .parse()
                            .map_err(|_| format!("Invalid value for --max-depth: {}", raw[i]))?,
                    );
                } else {
                    return Err("Missing argument for --max-depth".to_string());
                }
            }
            "--older-than" => {
                i += 1;
                if i < raw.len() {
                    older_than_days = Some(
                        raw[i]
                            .parse()
                            .map_err(|_| format!("Invalid value for --older-than: {}", raw[i]))?,
                    );
                } else {
                    return Err("Missing argument for --older-than".to_string());
                }
            }
            "--find-duplicates" => {
                find_duplicates = true;
            }
            "--interactive-clean" => {
                interactive_clean = true;
            }
            "--no-color" => {
                no_color = true;
            }
            "--json" => {
                i += 1;
                if i < raw.len() {
                    json_out = Some(raw[i].clone());
                } else {
                    return Err("Missing filename for --json".to_string());
                }
            }
            "--csv" => {
                i += 1;
                if i < raw.len() {
                    csv_out = Some(raw[i].clone());
                } else {
                    return Err("Missing filename for --csv".to_string());
                }
            }
            "--html" => {
                i += 1;
                if i < raw.len() {
                    html_out = Some(raw[i].clone());
                } else {
                    return Err("Missing filename for --html".to_string());
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
        exclude,
        json_out,
        csv_out,
        html_out,
        older_than_days,
        find_duplicates,
        max_depth,
        interactive_clean,
        no_color,
    })
}
