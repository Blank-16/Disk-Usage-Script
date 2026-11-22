mod cli;
mod diff;
mod duplicates;
mod export;
mod junk;
mod scanner;
#[cfg(test)]
mod tests;
mod types;
mod ui;
mod web_ui;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use cli::{enable_ansi_support, parse_args};
use diff::{compute_diff, load_snapshot, save_snapshot};
use duplicates::find_duplicate_groups;
use export::{export_csv, export_html_report, export_json};
use scanner::run_scan;
use types::Colors;
use ui::{
    handle_hardlink_deduplication, handle_interactive_clean, handle_junk_cleanup,
    pause_before_exit, print_age_distribution_table, print_diff_table, print_drive_summary,
    print_duplicates_table, print_extensions_table, print_junk_table, print_results_table,
    print_size_histogram,
};
use web_ui::start_gui_server;

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

    if args.gui {
        start_gui_server(args);
        return;
    }

    let raw_args: Vec<String> = std::env::args().collect();
    let mut args = args;
    if raw_args.len() == 1 && std::io::stdin().is_terminal() {
        println!(
            "================================================================================"
        );
        println!("                         DISK USAGE SCANNER");
        println!(
            "================================================================================"
        );
        println!("Select scan mode:");
        println!("  [1] Default Scan (Scan main drives with recommended settings)");
        println!("  [2] Advance Options (Configure custom drives, filters, depth, and junk)");
        println!("  [3] Open UI Window (Launch standalone dark-mode desktop window)");
        println!("  [4] Exit");
        println!(
            "--------------------------------------------------------------------------------"
        );
        print!("Select [1-4] (default: 1) > ");
        std::io::stdout().flush().ok();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        let choice = input.trim();

        if choice == "2" {
            print!("Enter drive(s) to scan (comma-separated, default: system drives): ");
            std::io::stdout().flush().ok();
            let mut drive_input = String::new();
            std::io::stdin().read_line(&mut drive_input).ok();
            let trimmed_d = drive_input.trim();
            if !trimmed_d.is_empty() {
                args.drives = trimmed_d
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }

            print!("Enable duplicate detection (y/N)? ");
            std::io::stdout().flush().ok();
            let mut dup_input = String::new();
            std::io::stdin().read_line(&mut dup_input).ok();
            if dup_input.trim().eq_ignore_ascii_case("y") {
                args.find_duplicates = true;
            }

            print!("Enable junk and cache detection (y/N)? ");
            std::io::stdout().flush().ok();
            let mut junk_input = String::new();
            std::io::stdin().read_line(&mut junk_input).ok();
            if junk_input.trim().eq_ignore_ascii_case("y") {
                args.find_junk = true;
            }

            print!("Show file size histogram (y/N)? ");
            std::io::stdout().flush().ok();
            let mut hist_input = String::new();
            std::io::stdin().read_line(&mut hist_input).ok();
            if hist_input.trim().eq_ignore_ascii_case("y") {
                args.size_histogram = true;
            }
        } else if choice == "3" {
            start_gui_server(args);
            return;
        } else if choice == "4"
            || choice.eq_ignore_ascii_case("q")
            || choice.eq_ignore_ascii_case("exit")
        {
            return;
        }
    }

    let colors = Colors::new(!args.no_color && ansi_supported);
    let is_interactive = std::io::stdout().is_terminal() && !args.quiet && ansi_supported;
    let mut valid_drives = Vec::new();

    for drive in &args.drives {
        let p = PathBuf::from(drive);
        if !p.exists() {
            println!(
                "{}Drive {:?} not found (skipping).{}",
                colors.red, drive, colors.reset
            );
            continue;
        }
        valid_drives.push(p);
    }

    if valid_drives.is_empty() {
        println!("{}No valid drives to scan.{}", colors.red, colors.reset);
        pause_before_exit(Some(&args));
        return;
    }

    println!();
    for drive in &valid_drives {
        print_drive_summary(&colors, drive);
    }

    let scan_results = run_scan(&args, &valid_drives, &colors, is_interactive);

    let files_table_title = if let Some(days) = args.older_than_days {
        format!("TOP {} LARGEST FILES (OLDER THAN {} DAYS)", args.top, days)
    } else {
        format!("TOP {} LARGEST FILES", args.top)
    };

    print_results_table(&colors, &files_table_title, &scan_results.largest_files);
    print_results_table(
        &colors,
        &format!("TOP {} LARGEST FOLDERS", args.top),
        &scan_results.largest_folders,
    );
    print_extensions_table(&colors, &scan_results.top_extensions);
    print_age_distribution_table(&colors, &scan_results.age_distribution);

    if args.size_histogram {
        print_size_histogram(&colors, &scan_results.size_histogram);
    }

    if args.find_junk || args.clean_junk {
        print_junk_table(&colors, &scan_results.junk_items);
        if args.clean_junk {
            handle_junk_cleanup(&scan_results.junk_items);
        }
    }

    let duplicate_groups = if args.find_duplicates {
        let dups = find_duplicate_groups(scan_results.size_candidates, args.top);
        print_duplicates_table(&colors, &dups);
        if args.dedupe_hardlink {
            handle_hardlink_deduplication(&dups);
        }
        dups
    } else {
        Vec::new()
    };

    if let Some(ref diff_path) = args.diff_snapshot {
        match load_snapshot(diff_path) {
            Ok(snap) => {
                let diffs = compute_diff(
                    &snap,
                    &scan_results.largest_folders,
                    &scan_results.largest_files,
                );
                print_diff_table(&colors, &diffs);
            }
            Err(e) => eprintln!("Failed to load snapshot from {}: {}", diff_path, e),
        }
    }

    if let Some(ref snap_path) = args.snapshot_out {
        match save_snapshot(
            snap_path,
            &scan_results.largest_files,
            &scan_results.largest_folders,
        ) {
            Ok(_) => println!("Saved scan snapshot to {}", snap_path),
            Err(e) => eprintln!("Failed to save snapshot to {}: {}", snap_path, e),
        }
    }

    println!();

    if let Some(ref json_path) = args.json_out {
        match export_json(
            json_path,
            scan_results.total_scanned,
            scan_results.total_elapsed,
            &scan_results.largest_files,
            &scan_results.largest_folders,
            &scan_results.top_extensions,
            &scan_results.age_distribution,
            &duplicate_groups,
        ) {
            Ok(_) => println!("Exported JSON to {}", json_path),
            Err(e) => eprintln!("Failed to export JSON to {}: {}", json_path, e),
        }
    }

    if let Some(ref csv_path) = args.csv_out {
        match export_csv(
            csv_path,
            &scan_results.largest_files,
            &scan_results.largest_folders,
            &scan_results.top_extensions,
            &scan_results.age_distribution,
        ) {
            Ok(_) => println!("Exported CSV to {}", csv_path),
            Err(e) => eprintln!("Failed to export CSV to {}: {}", csv_path, e),
        }
    }

    if let Some(ref html_path) = args.html_out {
        match export_html_report(
            html_path,
            scan_results.total_scanned,
            scan_results.total_elapsed,
            &scan_results.largest_files,
            &scan_results.largest_folders,
            &scan_results.top_extensions,
            &scan_results.age_distribution,
        ) {
            Ok(_) => println!("Exported interactive HTML report to {}", html_path),
            Err(e) => eprintln!("Failed to export HTML to {}: {}", html_path, e),
        }
    }

    if args.interactive_clean {
        handle_interactive_clean(&scan_results.largest_files);
    }

    pause_before_exit(Some(&args));
}
