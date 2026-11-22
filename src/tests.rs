#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use crate::diff::compute_diff;
    use crate::duplicates::fnv1a_64;
    use crate::export::escape_json;
    use crate::junk::match_junk_category;
    use crate::scanner::{get_path_depth, is_path_excluded, matches_filter};
    use crate::ui::{bytes_to_readable_size, format_number, format_proportion_bar};

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_bytes_to_readable_size() {
        assert_eq!(bytes_to_readable_size(50), "50 B");
        assert_eq!(bytes_to_readable_size(1024), "1.0 KB");
        assert_eq!(bytes_to_readable_size(1024 * 1024), "1.0 MB");
        assert_eq!(bytes_to_readable_size(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_format_proportion_bar() {
        assert_eq!(format_proportion_bar(0, 100), "[          ]");
        assert_eq!(format_proportion_bar(50, 100), "[=====     ]");
        assert_eq!(format_proportion_bar(100, 100), "[==========]");
    }

    #[test]
    fn test_fnv1a_64() {
        let hash1 = fnv1a_64(b"hello world");
        let hash2 = fnv1a_64(b"hello world");
        let hash3 = fnv1a_64(b"different content");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_matches_filter() {
        assert!(matches_filter("archive.zip", "*.zip"));
        assert!(matches_filter("image.png", "*.png"));
        assert!(!matches_filter("archive.tar.gz", "*.zip"));
        assert!(matches_filter("backup_2026.iso", "backup*"));
        assert!(matches_filter("my_report_final.pdf", "report"));
    }

    #[test]
    fn test_is_path_excluded() {
        let path = PathBuf::from("project/target/debug/app");
        let patterns = vec!["target".to_string(), "node_modules".to_string()];
        assert!(is_path_excluded(&path, &patterns));

        let clean_path = PathBuf::from("project/src/main.rs");
        assert!(!is_path_excluded(&clean_path, &patterns));
    }

    #[test]
    fn test_get_path_depth() {
        let root = PathBuf::from("/var/log");
        let deep = PathBuf::from("/var/log/nginx/access.log");
        assert_eq!(get_path_depth(&deep, &root), 2);
    }

    #[test]
    fn test_match_junk_category() {
        assert_eq!(
            match_junk_category("node_modules"),
            Some("Node.js Dependencies")
        );
        assert_eq!(match_junk_category("target"), Some("Rust/Build Artifacts"));
        assert_eq!(match_junk_category("__pycache__"), Some("Python Cache"));
        assert_eq!(match_junk_category("src"), None);
    }

    #[test]
    fn test_escape_json() {
        assert_eq!(escape_json("test \"quote\""), "test \\\"quote\\\"");
        assert_eq!(escape_json("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_json("c:\\path\\to"), "c:\\\\path\\\\to");
    }

    #[test]
    fn test_diff_computation() {
        let mut snapshot = HashMap::new();
        let path1 = PathBuf::from("/data/logs");
        snapshot.insert(path1.clone(), 1000);

        let current_folders = vec![(1500, path1.clone())];
        let current_files = vec![];

        let diffs = compute_diff(&snapshot, &current_folders, &current_files);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].delta, 500);
        assert_eq!(diffs[0].old_size, 1000);
        assert_eq!(diffs[0].new_size, 1500);
    }
}
