use std::cmp::Reverse;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn read_file_prefix_hash(path: &Path) -> Option<u64> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    let n = f.read(&mut buf).ok()?;
    Some(fnv1a_64(&buf[..n]))
}

pub fn read_full_file_hash(path: &Path) -> Option<u64> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 65536];
    let mut hash = 0xcbf29ce484222325u64;
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Some(hash)
}

pub fn find_duplicate_groups(
    size_candidates: HashMap<u64, Vec<PathBuf>>,
    top_n: usize,
) -> Vec<(u64, Vec<PathBuf>)> {
    let mut duplicate_groups = Vec::new();

    for (size, paths) in size_candidates {
        if paths.len() < 2 {
            continue;
        }

        let mut prefix_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Some(hash) = read_file_prefix_hash(&path) {
                prefix_groups.entry(hash).or_default().push(path);
            }
        }

        for (_, pgroup) in prefix_groups {
            if pgroup.len() < 2 {
                continue;
            }

            let mut full_groups: HashMap<u64, Vec<PathBuf>> = HashMap::new();
            for path in pgroup {
                if let Some(hash) = read_full_file_hash(&path) {
                    full_groups.entry(hash).or_default().push(path);
                }
            }

            for (_, fgroup) in full_groups {
                if fgroup.len() >= 2 {
                    let wasted = size * (fgroup.len() as u64 - 1);
                    duplicate_groups.push((wasted, fgroup));
                }
            }
        }
    }

    duplicate_groups.sort_by_key(|g| Reverse(g.0));
    if duplicate_groups.len() > top_n {
        duplicate_groups.truncate(top_n);
    }
    duplicate_groups
}
