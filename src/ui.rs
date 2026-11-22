use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::types::{Args, Colors, SEPARATOR_WIDTH};

#[cfg(unix)]
pub fn get_drive_space(path: &Path) -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_ulong};
    use std::os::unix::ffi::OsStrExt;

    #[repr(C)]
    struct Statvfs {
        f_bsize: c_ulong,
        f_frsize: c_ulong,
        f_blocks: c_ulong,
        f_bfree: c_ulong,
        f_bavail: c_ulong,
        f_files: c_ulong,
        f_ffree: c_ulong,
        f_favail: c_ulong,
        f_fsid: c_ulong,
        f_flag: c_ulong,
        f_namemax: c_ulong,
        f_spare: [c_int; 6],
    }

    extern "C" {
        fn statvfs(path: *const c_char, buf: *mut Statvfs) -> c_int;
    }

    let cpath = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf: Statvfs = unsafe { std::mem::zeroed() };
    let res = unsafe { statvfs(cpath.as_ptr(), &mut buf) };
    if res == 0 && buf.f_blocks > 0 {
        let block_size = if buf.f_frsize > 0 {
            buf.f_frsize as u64
        } else {
            buf.f_bsize as u64
        };
        let total = buf.f_blocks as u64 * block_size;
        let free = buf.f_bavail as u64 * block_size;
        let used = total.saturating_sub(free);
        Some((used, total))
    } else {
        None
    }
}

#[cfg(windows)]
pub fn get_drive_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    type Ulonglong = u64;
    type Bool = i32;
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut Ulonglong,
            lpTotalNumberOfBytes: *mut Ulonglong,
            lpTotalNumberOfFreeBytes: *mut Ulonglong,
        ) -> Bool;
    }

    let mut free_bytes: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free: u64 = 0;

    let res = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes,
            &mut total_bytes,
            &mut total_free,
        )
    };

    if res != 0 && total_bytes > 0 {
        let used = total_bytes.saturating_sub(free_bytes);
        Some((used, total_bytes))
    } else {
        None
    }
}

#[cfg(not(any(unix, windows)))]
pub fn get_drive_space(_path: &Path) -> Option<(u64, u64)> {
    None
}

pub fn format_number(mut n: u64) -> String {
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

pub fn bytes_to_readable_size(byte_count: u64) -> String {
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

pub fn format_proportion_bar(bytes: u64, max_bytes: u64) -> String {
    let ratio = if max_bytes > 0 {
        (bytes as f64 / max_bytes as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = (ratio * 10.0).round() as usize;
    let empty = 10usize.saturating_sub(filled);
    format!("[{}{}]", "=".repeat(filled), " ".repeat(empty))
}

pub fn print_drive_summary(colors: &Colors, path: &Path) {
    if let Some((used, total)) = get_drive_space(path) {
        let free = total.saturating_sub(used);
        let ratio = if total > 0 {
            (used as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pct = (ratio * 100.0) as u32;
        let filled = (ratio * 20.0).round() as usize;
        let empty = 20usize.saturating_sub(filled);
        let bar = format!("[{}{}]", "=".repeat(filled), " ".repeat(empty));
        println!(
            "{}Capacity for {}{}: {} {}% used ({} / {}, {} free)",
            colors.cyan,
            path.display(),
            colors.reset,
            bar,
            pct,
            bytes_to_readable_size(used),
            bytes_to_readable_size(total),
            bytes_to_readable_size(free)
        );
    }
}

pub fn print_results_table(colors: &Colors, table_title: &str, rows: &[(u64, PathBuf)]) {
    let separator = "=".repeat(SEPARATOR_WIDTH);
    println!("\n{}{}{}", colors.yellow, separator, colors.reset);
    println!(
        "{}{: <12}{} | {: <12} | {}{}{}",
        colors.green, "SIZE", colors.reset, "PROPORTION", colors.yellow, table_title, colors.reset
    );
    println!("{}{}{}", colors.yellow, separator, colors.reset);
    if rows.is_empty() {
        println!("  (No items found)");
    } else {
        let max_bytes = rows.first().map(|r| r.0).unwrap_or(1);
        for (i, (bytes, path)) in rows.iter().enumerate() {
            let size_str = bytes_to_readable_size(*bytes);
            let bar = format_proportion_bar(*bytes, max_bytes);
            println!(
                "{}{: <12}{} | {: <12} | [{}] {}",
                colors.green,
                size_str,
                colors.reset,
                bar,
                i + 1,
                path.display()
            );
        }
    }
}

pub fn print_extensions_table(colors: &Colors, rows: &[(String, u64, u64)]) {
    let separator = "=".repeat(SEPARATOR_WIDTH);
    println!("\n{}{}{}", colors.yellow, separator, colors.reset);
    println!(
        "{}{: <12}{} | {: <12} | {: <12} | {}TOP FILE EXTENSIONS BY USAGE{}",
        colors.green, "SIZE", colors.reset, "PROPORTION", "FILES", colors.yellow, colors.reset
    );
    println!("{}{}{}", colors.yellow, separator, colors.reset);
    if rows.is_empty() {
        println!("  (No file extensions found)");
    } else {
        let max_bytes = rows.first().map(|r| r.1).unwrap_or(1);
        for (ext, bytes, count) in rows {
            let size_str = bytes_to_readable_size(*bytes);
            let bar = format_proportion_bar(*bytes, max_bytes);
            let count_str = format_number(*count);
            println!(
                "{}{: <12}{} | {: <12} | {: <12} | {}",
                colors.green, size_str, colors.reset, bar, count_str, ext
            );
        }
    }
}

pub fn print_age_distribution_table(colors: &Colors, rows: &[(String, u64, u64)]) {
    let separator = "=".repeat(SEPARATOR_WIDTH);
    println!("\n{}{}{}", colors.yellow, separator, colors.reset);
    println!(
        "{}{: <12}{} | {: <12} | {: <12} | {}FILE AGE DISTRIBUTION{}",
        colors.green, "SIZE", colors.reset, "PROPORTION", "FILES", colors.yellow, colors.reset
    );
    println!("{}{}{}", colors.yellow, separator, colors.reset);
    let max_bytes = rows.iter().map(|r| r.1).max().unwrap_or(1);
    for (label, bytes, count) in rows {
        let size_str = bytes_to_readable_size(*bytes);
        let bar = format_proportion_bar(*bytes, max_bytes);
        let count_str = format_number(*count);
        println!(
            "{}{: <12}{} | {: <12} | {: <12} | {}",
            colors.green, size_str, colors.reset, bar, count_str, label
        );
    }
}

pub fn print_duplicates_table(colors: &Colors, rows: &[(u64, Vec<PathBuf>)]) {
    let separator = "=".repeat(SEPARATOR_WIDTH);
    println!("\n{}{}{}", colors.yellow, separator, colors.reset);
    println!(
        "{}{: <12}{} | {: <8} | {}TOP DUPLICATE FILES{}",
        colors.green, "WASTED", colors.reset, "COPIES", colors.yellow, colors.reset
    );
    println!("{}{}{}", colors.yellow, separator, colors.reset);
    if rows.is_empty() {
        println!("  (No duplicate files found)");
    } else {
        for (wasted, paths) in rows {
            let wasted_str = bytes_to_readable_size(*wasted);
            println!(
                "{}{: <12}{} | {: <8} | {}",
                colors.green,
                wasted_str,
                colors.reset,
                paths.len(),
                paths
                    .first()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            );
            for p in paths.iter().skip(1) {
                println!("{: <12}   {: <8}   | {}", "", "", p.display());
            }
        }
    }
}

pub fn handle_interactive_clean(largest_files: &[(u64, PathBuf)]) {
    if largest_files.is_empty() {
        return;
    }

    println!("\nInteractive Cleanup Mode:");
    println!(
        "Enter file number [1-{}] to delete, or press Enter to finish:",
        largest_files.len()
    );

    loop {
        print!("Select file > ");
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("q")
            || trimmed.eq_ignore_ascii_case("exit")
        {
            break;
        }

        if let Ok(choice) = trimmed.parse::<usize>() {
            if (1..=largest_files.len()).contains(&choice) {
                let target_path = &largest_files[choice - 1].1;
                print!(
                    "Are you sure you want to permanently delete '{}' (y/N)? ",
                    target_path.display()
                );
                io::stdout().flush().ok();

                let mut confirm = String::new();
                if io::stdin().read_line(&mut confirm).is_ok() {
                    let c = confirm.trim();
                    if c.eq_ignore_ascii_case("y") || c.eq_ignore_ascii_case("yes") {
                        match std::fs::remove_file(target_path) {
                            Ok(_) => println!("Successfully deleted {}", target_path.display()),
                            Err(e) => {
                                eprintln!("Failed to delete {}: {}", target_path.display(), e)
                            }
                        }
                    } else {
                        println!("Cancelled deletion.");
                    }
                }
            } else {
                println!(
                    "Invalid selection. Enter a number between 1 and {}.",
                    largest_files.len()
                );
            }
        } else {
            println!("Invalid input. Enter a number or press Enter to exit.");
        }
    }
}

pub fn pause_before_exit(args: Option<&Args>) {
    let should_pause = match args {
        Some(a) if a.no_pause => false,
        Some(a) if a.pause => true,
        _ => io::stdin().is_terminal(),
    };

    if should_pause {
        println!("\nPress Enter to exit...");
        io::stdout().flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).ok();
    }
}
