use crate::core::config::DedupConfig;
use crate::core::types::KeepStrategy;
use crate::models::media::{HashInfo, MediaItem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub content_id: String,
    pub items: Vec<DuplicateItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicateItem {
    pub index: usize,
    pub quality_score: f64,
    pub is_keep: bool,
}

pub struct Deduplicator {
    config: DedupConfig,
}

impl Deduplicator {
    pub fn new(config: DedupConfig) -> Self {
        Self { config }
    }

    /// Analyze items for duplicates, return duplicate groups
    pub fn analyze(&self, items: &[MediaItem]) -> Vec<DuplicateGroup> {
        let mut groups: Vec<DuplicateGroup> = Vec::new();

        // 1. Exact dedup: group by full_hash
        let mut hash_groups: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            if let Some(HashInfo { full_hash: Some(h), .. }) = &item.hash {
                hash_groups.entry(*h).or_default().push(i);
            }
        }

        for (hash, indices) in hash_groups {
            if indices.len() < 2 {
                continue;
            }
            groups.push(self.build_group(&format!("hash:{hash}"), &indices, items));
        }

        // 2. Version dedup: group by title + season/episode
        let mut content_groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            let key = if let Some(scraped) = &item.scraped {
                format!("{}|S{}E{}", scraped.title,
                    scraped.season_number.unwrap_or(0),
                    scraped.episode_number.unwrap_or(0))
            } else if let Some(parsed) = &item.parsed {
                format!("{}|S{}E{}", parsed.raw_title,
                    parsed.season.unwrap_or(0),
                    parsed.episode.unwrap_or(0))
            } else {
                continue;
            };
            content_groups.entry(key).or_default().push(i);
        }

        for (content_key, indices) in content_groups {
            if indices.len() < 2 {
                continue;
            }
            // Skip if already covered by exact dedup
            let hashes: Vec<Option<u64>> = indices.iter().map(|&i| {
                items[i].hash.as_ref().and_then(|h| h.full_hash)
            }).collect();
            let all_same = hashes.windows(2).all(|w| w[0].is_some() && w[0] == w[1]);
            if all_same {
                continue;
            }
            groups.push(self.build_group(&content_key, &indices, items));
        }

        groups
    }

    /// Execute dedup actions (dry-run supported)
    pub async fn execute(&self, groups: &[DuplicateGroup], items: &[MediaItem], dry_run: bool) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut actions: Vec<String> = Vec::new();

        for group in groups {
            for item in &group.items {
                if item.is_keep {
                    continue;
                }
                let media = &items[item.index];
                let path = &media.path;
                let action_desc = format!("REMOVE {} (score: {:.1})", path.display(), item.quality_score);

                if dry_run {
                    actions.push(format!("[dry-run] {action_desc}"));
                } else {
                    match self.config.duplicate_action {
                        crate::core::types::DupAction::Trash => {
                            match trash::delete(path) {
                                Ok(()) => {
                                    let msg = format!("[trash] {action_desc}");
                                    crate::core::oplog::log(&msg);
                                    actions.push(msg);
                                }
                                Err(e) => actions.push(format!("[error] failed to trash {}: {e}", path.display())),
                            }
                        }
                        crate::core::types::DupAction::Move => {
                            if self.config.move_target.as_os_str().is_empty() {
                                actions.push(format!("[skip] no move_target configured for {}", path.display()));
                            } else {
                                let dest = self.config.move_target.join(path.file_name().unwrap_or_default());
                                match std::fs::rename(path, &dest) {
                                    Ok(()) => {
                                        let msg = format!("[move] {} → {}", path.display(), dest.display());
                                        crate::core::oplog::log(&msg);
                                        actions.push(msg);
                                    }
                                    Err(e) => actions.push(format!("[error] failed to move {}: {e}", path.display())),
                                }
                            }
                        }
                        crate::core::types::DupAction::Report => {
                            actions.push(format!("[report] {action_desc}"));
                        }
                    }
                }
            }
        }

        Ok(actions)
    }

    fn build_group(&self, content_id: &str, indices: &[usize], items: &[MediaItem]) -> DuplicateGroup {
        let mut dup_items: Vec<DuplicateItem> = indices
            .iter()
            .map(|&i| {
                let score = items[i].quality.as_ref().map(|q| q.quality_score).unwrap_or(0.0);
                DuplicateItem {
                    index: i,
                    quality_score: score,
                    is_keep: false,
                }
            })
            .collect();

        // Determine which to keep
        let keep_idx = match self.config.keep_strategy {
            KeepStrategy::HighestQuality => {
                dup_items.iter().enumerate().max_by(|a, b| a.1.quality_score.partial_cmp(&b.1.quality_score).unwrap()).map(|(i, _)| i)
            }
            KeepStrategy::Newest => {
                // Pre-fetch modified times to avoid repeated fs::metadata calls
                let mod_times: Vec<Option<std::time::SystemTime>> = dup_items.iter()
                    .map(|d| std::fs::metadata(&items[d.index].path).and_then(|m| m.modified()).ok())
                    .collect();
                mod_times.iter().enumerate()
                    .filter(|(_, t)| t.is_some())
                    .max_by_key(|(_, t)| t.unwrap())
                    .map(|(i, _)| i)
            }
            KeepStrategy::Largest => {
                dup_items.iter().enumerate().max_by_key(|(_, d)| items[d.index].file_size).map(|(i, _)| i)
            }
            KeepStrategy::Manual => None,
        };

        if let Some(ki) = keep_idx {
            dup_items[ki].is_keep = true;
        }

        DuplicateGroup {
            content_id: content_id.to_string(),
            items: dup_items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::DedupConfig;
    use crate::core::types::{DupAction, KeepStrategy};
    use crate::models::media::{MediaItem, MediaType};
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    fn make_item(path: PathBuf, full_hash: u64) -> MediaItem {
        MediaItem {
            id: 0,
            path,
            file_size: 1024,
            media_type: MediaType::Movie,
            extension: "mkv".into(),
            parsed: None,
            quality: None,
            scraped: None,
            hash: Some(HashInfo {
                size_hash: full_hash,
                prefix_hash: Some(full_hash),
                full_hash: Some(full_hash),
            }),
            rename_plan: None,
        }
    }

    #[test]
    fn test_keep_strategy_newest_prefers_newer_file() {
        let dir = tempfile::tempdir().unwrap();
        let older_path = dir.path().join("older.mkv");
        let newer_path = dir.path().join("newer.mkv");

        let mut older = std::fs::File::create(&older_path).unwrap();
        writeln!(older, "older").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let mut newer = std::fs::File::create(&newer_path).unwrap();
        writeln!(newer, "newer").unwrap();

        let items = vec![make_item(older_path, 42), make_item(newer_path, 42)];
        let deduplicator = Deduplicator::new(DedupConfig {
            hash_algorithm: "xxhash".into(),
            keep_strategy: KeepStrategy::Newest,
            duplicate_action: DupAction::Report,
            move_target: PathBuf::new(),
        });

        let groups = deduplicator.analyze(&items);
        assert_eq!(groups.len(), 1);
        let keep_index = groups[0]
            .items
            .iter()
            .find(|item| item.is_keep)
            .map(|item| item.index)
            .unwrap();
        assert_eq!(items[keep_index].path.file_name().unwrap(), "newer.mkv");
    }
}
