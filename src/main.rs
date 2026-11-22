mod cli;
mod duplicates;
mod export;
mod scanner;
mod types;
mod ui;

use std::io::IsTerminal;
use std::path::PathBuf;

use cli::{enable_ansi_support, parse_args};
use duplicates::find_duplicate_groups;
use export::{export_csv, export_html_report, export_json};
use scanner::run_scan;
use types::Colors;
use ui::{
    handle_interactive_clean, pause_before_exit, print_age_distribution_table, print_drive_summary,
    print_duplicates_table, print_extensions_table, print_results_table,
};

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

    let duplicate_groups = if args.find_duplicates {
        let dups = find_duplicate_groups(scan_results.size_candidates, args.top);
        print_duplicates_table(&colors, &dups);
        dups
    } else {
        Vec::new()
    };

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
