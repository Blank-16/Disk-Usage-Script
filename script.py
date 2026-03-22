import os
import heapq
import sys
import time
import argparse

# ANSI COLOR CODES
COLOR_CYAN   = "\033[96m"
COLOR_GREEN  = "\033[92m"
COLOR_YELLOW = "\033[93m"
COLOR_RED    = "\033[91m"
COLOR_RESET  = "\033[0m"
COLOR_DIM    = "\033[2m"

# DEFAULT SETTINGS

DEFAULT_DRIVES_TO_SCAN    = ["C:\\", "D:\\"]
DEFAULT_RESULTS_TO_SHOW   = 20
MINIMUM_FOLDER_SIZE_BYTES = 1024 ** 2  # 1 MB — skip folders smaller than this


# Helpers

def bytes_to_readable_size(byte_count: int) -> str:
    """Convert a raw byte count to a human-readable GB or MB string."""
    size_in_gb = byte_count / (1024 ** 3)
    if size_in_gb >= 0.01:
        return f"{size_in_gb:.2f} GB"
    size_in_mb = byte_count / (1024 ** 2)
    return f"{size_in_mb:.1f} MB"


def print_results_table(table_title: str, size_and_path_rows: list[tuple[int, str]]) -> None:
    """Print a formatted two-column table showing size and path for each row."""
    separator_width = 80
    print(f"\n{COLOR_YELLOW}{'=' * separator_width}{COLOR_RESET}")
    print(f"{COLOR_GREEN}{'SIZE':<12}{COLOR_RESET} | {COLOR_YELLOW}{table_title}{COLOR_RESET}")
    print(f"{COLOR_YELLOW}{'=' * separator_width}{COLOR_RESET}")
    for byte_count, file_path in size_and_path_rows:
        print(f"{COLOR_GREEN}{bytes_to_readable_size(byte_count):<12}{COLOR_RESET} | {file_path}")


# Core scanner

def find_largest_files_and_folders(
    drives_to_scan: list[str],
    number_of_results: int,
    hide_progress: bool = False,
) -> tuple[list, list]:
    """
    Walk each drive and track the largest files and folders using min-heaps.
    Returns two sorted lists: (largest_files, largest_folders), biggest first.
    """
    largest_files:   list[tuple[int, str]] = []  # min-heap of (size, path)
    largest_folders: list[tuple[int, str]] = []  # min-heap of (size, path)
    total_files_scanned = 0
    scan_start_time = time.monotonic()
    recent_paths = []  # rolling list of last 10 scanned paths
    MAX_RECENT_PATHS = 10

    for drive in drives_to_scan:
        if not os.path.exists(drive):
            print(f"{COLOR_RED}Drive {drive!r} not found – skipping.{COLOR_RESET}")
            continue

        print(f"\n{COLOR_YELLOW}--- Starting Scan on Drive {drive} ---{COLOR_RESET}")

        for current_folder, subfolders, filenames in os.walk(drive):
            # Check we can actually read this folder before going deeper
            try:
                os.scandir(current_folder)
            except (PermissionError, OSError):
                subfolders.clear()  # prevent os.walk from descending into children
                continue

            current_folder_total_size = 0

            for filename in filenames:
                try:
                    full_file_path = os.path.join(current_folder, filename)
                    if os.path.islink(full_file_path):
                        continue  # skip symlinks — they don't represent real disk usage
                    file_size_in_bytes = os.path.getsize(full_file_path)
                    if file_size_in_bytes == 0:
                        continue  # skip empty files — not useful in a "largest" list

                    total_files_scanned += 1
                    current_folder_total_size += file_size_in_bytes

                    # Keep only the top N largest files using a min-heap
                    if len(largest_files) < number_of_results:
                        heapq.heappush(largest_files, (file_size_in_bytes, full_file_path))
                    else:
                        heapq.heappushpop(largest_files, (file_size_in_bytes, full_file_path))

                    # Update the rolling display of recent paths
                    if not hide_progress:
                        recent_paths.append(full_file_path)
                        if len(recent_paths) > MAX_RECENT_PATHS:
                            recent_paths.pop(0)
                        
                        # Clear previous display and redraw
                        elapsed_seconds = time.monotonic() - scan_start_time
                        # Move cursor up to clear previous lines (only after first iteration)
                        if total_files_scanned > 1:
                            sys.stdout.write(f"\033[{MAX_RECENT_PATHS + 2}A")  # Move up
                        sys.stdout.write("\033[J")  # Clear from cursor to end of screen
                        
                        # Print header
                        print(f"{COLOR_CYAN}[{elapsed_seconds:6.1f}s | {total_files_scanned:,} files scanned]{COLOR_RESET}")
                        print(f"{COLOR_DIM}Recently scanned (last {len(recent_paths)}):{COLOR_RESET}")
                        
                        # Print the last 10 paths
                        for path in recent_paths:
                            truncated_path = path[:75] + "..." if len(path) > 75 else path
                            print(f"{COLOR_GREEN}• {COLOR_RESET}{truncated_path}")
                        
                        # Fill remaining lines to maintain consistent height
                        for _ in range(MAX_RECENT_PATHS - len(recent_paths)):
                            print()
                        
                        sys.stdout.flush()

                except (PermissionError, OSError):
                    continue

            # Only track folders large enough to be worth reporting
            if current_folder_total_size >= MINIMUM_FOLDER_SIZE_BYTES:
                if len(largest_folders) < number_of_results:
                    heapq.heappush(largest_folders, (current_folder_total_size, current_folder))
                else:
                    heapq.heappushpop(largest_folders, (current_folder_total_size, current_folder))

    total_elapsed_seconds = time.monotonic() - scan_start_time

    if not hide_progress:
        # Clear the progress display
        sys.stdout.write(f"\033[{MAX_RECENT_PATHS + 2}A")
        sys.stdout.write("\033[J")

    print(
        f"{COLOR_DIM}Scan complete: {total_files_scanned:,} files across "
        f"{len(drives_to_scan)} drive(s) in {total_elapsed_seconds:.1f}s{COLOR_RESET}"
    )

    return sorted(largest_files, reverse=True), sorted(largest_folders, reverse=True)


# Entry point

def parse_command_line_arguments() -> argparse.Namespace:
    argument_parser = argparse.ArgumentParser(
        description="Scan drives and report the largest files and folders."
    )
    argument_parser.add_argument(
        "--drives",
        nargs="+",
        default=DEFAULT_DRIVES_TO_SCAN,
        metavar="DRIVE",
        help=f"Drives to scan (default: {' '.join(DEFAULT_DRIVES_TO_SCAN)})",
    )
    argument_parser.add_argument(
        "--top",
        type=int,
        default=DEFAULT_RESULTS_TO_SHOW,
        metavar="N",
        help=f"How many results to show (default: {DEFAULT_RESULTS_TO_SHOW})",
    )
    argument_parser.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress the live progress line (useful when piping output)",
    )
    return argument_parser.parse_args()


if __name__ == "__main__":
    os.system("")  # enable ANSI colours in Windows CMD

    user_arguments = parse_command_line_arguments()

    largest_files, largest_folders = find_largest_files_and_folders(
        drives_to_scan=user_arguments.drives,
        number_of_results=user_arguments.top,
        hide_progress=user_arguments.quiet,
    )

    files_table_title   = f"TOP {user_arguments.top} LARGEST FILES"
    folders_table_title = f"TOP {user_arguments.top} LARGEST FOLDERS"

    print_results_table(files_table_title,   largest_files)
    print_results_table(folders_table_title, largest_folders)
    print()
