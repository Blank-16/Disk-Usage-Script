pub const DEFAULT_RESULTS_TO_SHOW: usize = 20;
pub const MINIMUM_FOLDER_SIZE_BYTES: u64 = 1024 * 1024;
pub const SEPARATOR_WIDTH: usize = 80;

pub struct Colors {
    pub cyan: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub red: &'static str,
    pub reset: &'static str,
    pub dim: &'static str,
}

impl Colors {
    pub fn new(enabled: bool) -> Self {
        if enabled {
            Colors {
                cyan: "\x1b[96m",
                green: "\x1b[92m",
                yellow: "\x1b[93m",
                red: "\x1b[91m",
                reset: "\x1b[0m",
                dim: "\x1b[2m",
            }
        } else {
            Colors {
                cyan: "",
                green: "",
                yellow: "",
                red: "",
                reset: "",
                dim: "",
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Args {
    pub drives: Vec<String>,
    pub top: usize,
    pub quiet: bool,
    pub threads: usize,
    pub min_folder_size: u64,
    pub pause: bool,
    pub no_pause: bool,
    pub exclude: Vec<String>,
    pub json_out: Option<String>,
    pub csv_out: Option<String>,
    pub html_out: Option<String>,
    pub older_than_days: Option<u64>,
    pub find_duplicates: bool,
    pub max_depth: Option<usize>,
    pub interactive_clean: bool,
    pub no_color: bool,
    pub snapshot_out: Option<String>,
    pub diff_snapshot: Option<String>,
    pub find_junk: bool,
    pub clean_junk: bool,
    pub size_histogram: bool,
    pub filter_pattern: Option<String>,
    pub dedupe_hardlink: bool,
}
