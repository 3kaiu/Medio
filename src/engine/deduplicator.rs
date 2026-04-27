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
                                Ok(()) => actions.push(format!("[trash] {action_desc}")),
                                Err(e) => actions.push(format!("[error] failed to trash {}: {e}", path.display())),
                            }
                        }
                        crate::core::types::DupAction::Move => {
                            if self.config.move_target.as_os_str().is_empty() {
                                actions.push(format!("[skip] no move_target configured for {}", path.display()));
                            } else {
                                let dest = self.config.move_target.join(path.file_name().unwrap_or_default());
                                match std::fs::rename(path, &dest) {
                                    Ok(()) => actions.push(format!("[move] {} → {}", path.display(), dest.display())),
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
                // Sort by file modified time — use path as proxy for now
                Some(0)
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
