use crate::models::media::{HashInfo, MediaItem};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct FileHasher;

impl FileHasher {
    /// Stage 1: Group items by file size — same size are potential duplicates
    pub fn group_by_size(items: &[MediaItem]) -> Vec<Vec<usize>> {
        let mut size_map: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            size_map.entry(item.file_size).or_default().push(i);
        }
        size_map.into_values().filter(|g| g.len() >= 2).collect()
    }

    /// Stage 2: Compute prefix hash (first 64KB) for items in groups
    pub fn prefix_hash(paths: &[&Path]) -> Vec<Option<u64>> {
        paths.par_iter().map(|p| Self::hash_prefix(p)).collect()
    }

    /// Stage 3: Compute full file hash
    #[allow(dead_code)]
    pub fn full_hash(paths: &[&Path]) -> Vec<Option<u64>> {
        paths.par_iter().map(|p| Self::hash_full(p)).collect()
    }

    /// Compute progressive hashes for all items, updating HashInfo
    pub fn compute_all(items: &mut [MediaItem]) {
        let groups = Self::group_by_size(items);

        let mut prefix_results: HashMap<usize, u64> = HashMap::new();
        for group in &groups {
            let paths: Vec<&Path> = group.iter().map(|&i| items[i].path.as_path()).collect();
            let hashes = Self::prefix_hash(&paths);
            for (j, &idx) in group.iter().enumerate() {
                if let Some(h) = hashes[j] {
                    prefix_results.insert(idx, h);
                }
            }
        }

        let mut need_full: Vec<Vec<usize>> = Vec::new();
        for group in &groups {
            let mut prefix_groups: HashMap<u64, Vec<usize>> = HashMap::new();
            for &idx in group {
                if let Some(h) = prefix_results.get(&idx) {
                    prefix_groups.entry(*h).or_default().push(idx);
                }
            }
            for (_, same_prefix) in prefix_groups {
                if same_prefix.len() >= 2 {
                    need_full.push(same_prefix);
                }
            }
        }

        let mut full_results: HashMap<usize, u64> = HashMap::new();
        for group in &need_full {
            let paths: Vec<&Path> = group.iter().map(|&i| items[i].path.as_path()).collect();
            let hashes = Self::hash_full_batch(&paths);
            for (j, &idx) in group.iter().enumerate() {
                if let Some(h) = hashes[j] {
                    full_results.insert(idx, h);
                }
            }
        }

        for (i, item) in items.iter_mut().enumerate() {
            let size_hash = item.file_size;
            let prefix_hash = prefix_results.get(&i).copied();
            let full_hash = full_results.get(&i).copied();

            if prefix_hash.is_some() || full_hash.is_some() {
                item.hash = Some(HashInfo { size_hash, prefix_hash, full_hash });
            }
        }
    }

    fn hash_prefix(path: &Path) -> Option<u64> {
        let mut file = File::open(path).ok()?;
        let mut buf = vec![0u8; 65536];
        let n = file.read(&mut buf).ok()?;
        buf.truncate(n);
        let mut hasher = twox_hash::XxHash64::default();
        use std::hash::Hasher;
        hasher.write(&buf);
        Some(hasher.finish())
    }

    fn hash_full(path: &Path) -> Option<u64> {
        let mut file = File::open(path).ok()?;
        let mut hasher = twox_hash::XxHash64::default();
        use std::hash::Hasher;
        let mut buf = vec![0u8; 8192];
        loop {
            let n = file.read(&mut buf).ok()?;
            if n == 0 { break; }
            hasher.write(&buf[..n]);
        }
        Some(hasher.finish())
    }

    fn hash_full_batch(paths: &[&Path]) -> Vec<Option<u64>> {
        paths.par_iter().map(|p| Self::hash_full(p)).collect()
    }
}
