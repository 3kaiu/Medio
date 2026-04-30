use crate::db::cache::Cache;
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

    pub fn compute_all_with_cache(items: &mut [MediaItem], cache: Option<&Cache>) {
        if let Some(cache) = cache {
            for item in items.iter_mut() {
                let cache_key = Self::cache_key_for_item(item);
                if let Some(hash) = cache.get_hash(&cache_key) {
                    item.hash = Some(hash);
                }
            }
        }

        let groups = Self::group_by_size(items);

        // Merge all unresolved prefix hash paths into a single batch for better rayon utilization
        let mut prefix_unresolved: Vec<usize> = Vec::new();
        for group in &groups {
            for &idx in group {
                if items[idx]
                    .hash
                    .as_ref()
                    .and_then(|h| h.prefix_hash)
                    .is_none()
                {
                    prefix_unresolved.push(idx);
                }
            }
        }

        let mut prefix_results: HashMap<usize, u64> = HashMap::new();
        if !prefix_unresolved.is_empty() {
            let paths: Vec<&Path> = prefix_unresolved
                .iter()
                .map(|&i| items[i].path.as_path())
                .collect();
            let hashes = Self::prefix_hash(&paths);
            for (j, &idx) in prefix_unresolved.iter().enumerate() {
                if let Some(h) = hashes[j] {
                    prefix_results.insert(idx, h);
                }
            }
        }

        let mut need_full: Vec<Vec<usize>> = Vec::new();
        for group in &groups {
            let mut prefix_groups: HashMap<u64, Vec<usize>> = HashMap::new();
            for &idx in group {
                let prefix_hash = items[idx]
                    .hash
                    .as_ref()
                    .and_then(|h| h.prefix_hash)
                    .or_else(|| prefix_results.get(&idx).copied());
                if let Some(h) = prefix_hash {
                    prefix_groups.entry(h).or_default().push(idx);
                }
            }
            for (_, same_prefix) in prefix_groups {
                let missing_full = same_prefix
                    .iter()
                    .any(|&idx| items[idx].hash.as_ref().and_then(|h| h.full_hash).is_none());
                if same_prefix.len() >= 2 && missing_full {
                    need_full.push(same_prefix);
                }
            }
        }

        // Merge all unresolved full hash paths into a single batch
        let mut full_unresolved: Vec<usize> = Vec::new();
        for group in &need_full {
            for &idx in group {
                if items[idx].hash.as_ref().and_then(|h| h.full_hash).is_none() {
                    full_unresolved.push(idx);
                }
            }
        }

        let mut full_results: HashMap<usize, u64> = HashMap::new();
        if !full_unresolved.is_empty() {
            let paths: Vec<&Path> = full_unresolved
                .iter()
                .map(|&i| items[i].path.as_path())
                .collect();
            let hashes = Self::hash_full_batch(&paths);
            for (j, &idx) in full_unresolved.iter().enumerate() {
                if let Some(h) = hashes[j] {
                    full_results.insert(idx, h);
                }
            }
        }

        for (i, item) in items.iter_mut().enumerate() {
            let size_hash = item.file_size;
            let prefix_hash = item
                .hash
                .as_ref()
                .and_then(|h| h.prefix_hash)
                .or_else(|| prefix_results.get(&i).copied());
            let full_hash = item
                .hash
                .as_ref()
                .and_then(|h| h.full_hash)
                .or_else(|| full_results.get(&i).copied());

            if prefix_hash.is_some() || full_hash.is_some() {
                let hash_info = HashInfo {
                    size_hash,
                    prefix_hash,
                    full_hash,
                };
                if let Some(cache) = cache {
                    let cache_key = Self::cache_key_for_item(item);
                    let _ = cache.set_hash(&cache_key, &hash_info);
                }
                item.hash = Some(hash_info);
            }
        }

        if let Some(cache) = cache {
            let _ = cache.flush();
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
            if n == 0 {
                break;
            }
            hasher.write(&buf[..n]);
        }
        Some(hasher.finish())
    }

    fn hash_full_batch(paths: &[&Path]) -> Vec<Option<u64>> {
        paths.par_iter().map(|p| Self::hash_full(p)).collect()
    }

    fn cache_key_for_item(item: &MediaItem) -> String {
        let modified = std::fs::metadata(&item.path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|ts| ts.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{}:{}:{}", item.path.display(), item.file_size, modified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_item(path: &Path) -> MediaItem {
        let size = std::fs::metadata(path).unwrap().len();
        MediaItem {
            id: 0,
            path: path.to_path_buf(),
            file_size: size,
            media_type: crate::models::media::MediaType::Movie,
            extension: path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            parsed: None,
            quality: None,
            scraped: None,
            content_evidence: None,
            identity_resolution: None,
            hash: None,
            rename_plan: None,
        }
    }

    #[test]
    fn test_compute_all_uses_cached_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sled")).unwrap();
        let file_a = dir.path().join("a.mkv");
        let file_b = dir.path().join("b.mkv");
        let mut a = std::fs::File::create(&file_a).unwrap();
        let mut b = std::fs::File::create(&file_b).unwrap();
        writeln!(a, "same-content").unwrap();
        writeln!(b, "same-content").unwrap();

        let mut items = vec![make_item(&file_a), make_item(&file_b)];
        FileHasher::compute_all_with_cache(&mut items, Some(&cache));
        let first_hash = items[0].hash.clone().unwrap();

        let mut items_again = vec![make_item(&file_a), make_item(&file_b)];
        FileHasher::compute_all_with_cache(&mut items_again, Some(&cache));
        assert_eq!(
            items_again[0].hash.as_ref().unwrap().full_hash,
            first_hash.full_hash
        );
        assert_eq!(
            items_again[1].hash.as_ref().unwrap().full_hash,
            first_hash.full_hash
        );
    }
}
