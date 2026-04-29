use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::hasher::FileHasher;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::db::cache::Cache;
use crate::engine::deduplicator::{Deduplicator, DuplicateGroup};
use crate::engine::organizer::{OrganizePlan, Organizer};
use crate::engine::renamer::Renamer;
use crate::media::native_probe::NativeProbe;
use crate::media::probe::MediaProbe;
use crate::models::media::MediaItem;
use crate::scraper;
use std::cell::RefCell;
#[cfg(test)]
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tab {
    Scan,
    Dedup,
    Rename,
    Organize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Confirm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingAction {
    Rename,
    Dedup,
    Organize,
}

struct FilteredCache {
    cache_gen: u64,
    query: String,
    tab: Tab,
    items_len: usize,
    filtered_items: Vec<(usize, usize)>, // (original index, selected index)
}

impl Default for FilteredCache {
    fn default() -> Self {
        Self {
            cache_gen: 0,
            query: String::new(),
            tab: Tab::Scan,
            items_len: 0,
            filtered_items: Vec::new(),
        }
    }
}

pub struct App {
    pub config: AppConfig,
    pub items: Vec<MediaItem>,
    pub dedup_groups: Vec<DuplicateGroup>,
    pub rename_plans: Vec<crate::models::media::RenamePlan>,
    pub organize_plans: Vec<OrganizePlan>,
    pub tab: Tab,
    pub mode: Mode,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search_query: String,
    pub status_msg: String,
    pub should_quit: bool,
    pub path: String,
    pub pending_action: Option<PendingAction>,
    pub view_mode: ViewMode,
    data_gen: u64,
    cache: RefCell<FilteredCache>,
}

impl App {
    pub fn new(config: AppConfig, path: String) -> Self {
        Self {
            config,
            items: Vec::new(),
            dedup_groups: Vec::new(),
            rename_plans: Vec::new(),
            organize_plans: Vec::new(),
            tab: Tab::Scan,
            mode: Mode::Normal,
            selected: 0,
            scroll_offset: 0,
            search_query: String::new(),
            status_msg: "Press 's' to scan, Tab to switch views, q to quit".into(),
            should_quit: false,
            path,
            pending_action: None,
            view_mode: ViewMode::Table,
            data_gen: 0,
            cache: RefCell::new(FilteredCache::default()),
        }
    }

    pub fn scan(&mut self) {
        let scanner = Scanner::new(self.config.scan.clone());
        let path = std::path::Path::new(&self.path);
        if !path.exists() {
            self.status_msg = format!("Path not found: {}", self.path);
            return;
        }

        self.items = scanner.scan(path);
        self.data_gen += 1;

        if self.items.is_empty() {
            self.status_msg = "No media files found.".into();
            return;
        }

        // Identify
        let keyword_filter = KeywordFilter::new(self.config.scan.keyword_filter.clone());
        let identifier = Identifier::new(keyword_filter);
        identifier.parse_batch(&mut self.items);

        // Context inference
        for item in self.items.iter_mut() {
            ContextInfer::enrich_item(item);
        }

        // Shared scrape path
        let rt = match crate::core::runtime::build() {
            Ok(rt) => rt,
            Err(err) => {
                self.status_msg = err;
                return;
            }
        };
        rt.block_on(async {
            scraper::populate_scrape_results(&mut self.items, &self.config).await;
        });

        // Hash and quality preview for dedup scoring
        let cache = Cache::open(&self.config.cache_path()).ok();
        if let Some(ref cache) = cache {
            let _ = cache.cleanup(self.config.cache.ttl_days);
        }
        FileHasher::compute_all_with_cache(&mut self.items, cache.as_ref());

        let probe = NativeProbe::new(self.config.quality.clone());
        for item in self.items.iter_mut() {
            if let Ok(quality) = probe.probe(&item.path) {
                item.quality = Some(quality);
            }
        }

        // Derived previews for other tabs
        let deduplicator = Deduplicator::new(self.config.dedup.clone());
        self.dedup_groups = deduplicator.analyze(&self.items);
        self.data_gen += 1;

        let renamer = Renamer::new(self.config.rename.clone());
        self.rename_plans = renamer.plan(&self.items);

        let organizer = Organizer::new(self.config.organize.clone());
        self.organize_plans = organizer.plan(
            &self.items,
            self.config.organize.mode,
            self.config.organize.link_mode,
        );

        self.selected = 0;
        self.scroll_offset = 0;
        self.status_msg = format!(
            "Scanned {} files | dedup:{} rename:{} organize:{}",
            self.items.len(),
            self.dedup_groups.len(),
            self.rename_plans.len(),
            self.organize_plans.len()
        );
    }

    pub fn next_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Scan => Tab::Dedup,
            Tab::Dedup => Tab::Rename,
            Tab::Rename => Tab::Organize,
            Tab::Organize => Tab::Scan,
        };
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn prev_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Scan => Tab::Organize,
            Tab::Dedup => Tab::Scan,
            Tab::Rename => Tab::Dedup,
            Tab::Organize => Tab::Rename,
        };
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn select_next(&mut self) {
        let len = self.current_len();
        if len > 0 && self.selected < len - 1 {
            self.selected += 1;
            self.adjust_scroll();
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.adjust_scroll();
        }
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn select_last(&mut self) {
        let len = self.current_len();
        if len > 0 {
            self.selected = len - 1;
            self.adjust_scroll();
        }
    }

    pub fn page_down(&mut self, page_size: usize) {
        let len = self.current_len();
        if len > 0 {
            self.selected = (self.selected + page_size).min(len - 1);
            self.adjust_scroll();
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
        self.adjust_scroll();
    }

    fn adjust_scroll(&mut self) {
        // Keep selected item visible
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
        // scroll_offset will be further adjusted in render based on visible height
    }

    #[allow(dead_code)]
    pub fn set_search(&mut self, query: String) {
        self.search_query = query;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn filtered_items(&self) -> Vec<(usize, &MediaItem)> {
        {
            let cache = self.cache.borrow();
            if cache.cache_gen == self.data_gen
                && cache.query == self.search_query
                && cache.tab == Tab::Scan
                && cache.items_len == self.items.len()
            {
                return cache
                    .filtered_items
                    .iter()
                    .map(|&(orig_idx, _)| (orig_idx, &self.items[orig_idx]))
                    .collect();
            }
        }

        let indices: Vec<(usize, usize)> = if self.search_query.is_empty() {
            self.items.iter().enumerate().map(|(i, _)| (i, i)).collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    let name = item
                        .path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    name.contains(&q)
                        || item
                            .parsed
                            .as_ref()
                            .map(|p| p.raw_title.to_lowercase().contains(&q))
                            .unwrap_or(false)
                        || item
                            .scraped
                            .as_ref()
                            .map(|s| s.title.to_lowercase().contains(&q))
                            .unwrap_or(false)
                })
                .enumerate()
                .map(|(sel, (orig, _))| (orig, sel))
                .collect()
        };

        let result: Vec<(usize, &MediaItem)> = indices
            .iter()
            .map(|&(orig_idx, _)| (orig_idx, &self.items[orig_idx]))
            .collect();

        let mut cache = self.cache.borrow_mut();
        cache.cache_gen = self.data_gen;
        cache.query = self.search_query.clone();
        cache.tab = Tab::Scan;
        cache.items_len = self.items.len();
        cache.filtered_items = indices;

        result
    }

    pub fn filtered_dedup_groups(&self) -> Vec<(usize, &DuplicateGroup)> {
        if self.search_query.is_empty() {
            self.dedup_groups.iter().enumerate().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.dedup_groups
                .iter()
                .enumerate()
                .filter(|(_, group)| {
                    group.content_id.to_lowercase().contains(&q)
                        || group.items.iter().any(|entry| {
                            self.items[entry.index]
                                .path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_lowercase().contains(&q))
                                .unwrap_or(false)
                        })
                })
                .collect()
        }
    }

    pub fn filtered_rename_plans(&self) -> Vec<(usize, &crate::models::media::RenamePlan)> {
        if self.search_query.is_empty() {
            self.rename_plans.iter().enumerate().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.rename_plans
                .iter()
                .enumerate()
                .filter(|(_, plan)| {
                    plan.old_path.to_string_lossy().to_lowercase().contains(&q)
                        || plan.new_path.to_string_lossy().to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn filtered_organize_plans(&self) -> Vec<(usize, &OrganizePlan)> {
        if self.search_query.is_empty() {
            self.organize_plans.iter().enumerate().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.organize_plans
                .iter()
                .enumerate()
                .filter(|(_, plan)| {
                    plan.source.to_string_lossy().to_lowercase().contains(&q)
                        || plan.target.to_string_lossy().to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn current_len(&self) -> usize {
        match self.tab {
            Tab::Scan => self.filtered_items().len(),
            Tab::Dedup => self.filtered_dedup_groups().len(),
            Tab::Rename => self.filtered_rename_plans().len(),
            Tab::Organize => self.filtered_organize_plans().len(),
        }
    }

    pub fn request_rename_execute(&mut self) {
        if self.rename_plans.is_empty() {
            self.status_msg = "No rename plans to execute".into();
            return;
        }
        self.pending_action = Some(PendingAction::Rename);
        self.mode = Mode::Confirm;
        self.status_msg = format!(
            "Execute {} rename plans? [Enter/y=yes, n/Esc=no]",
            self.rename_plans.len()
        );
    }

    pub fn request_dedup_execute(&mut self) {
        if self.dedup_groups.is_empty() {
            self.status_msg = "No duplicate groups to execute".into();
            return;
        }
        let remove_count = self
            .dedup_groups
            .iter()
            .map(|group| group.items.iter().filter(|item| !item.is_keep).count())
            .sum::<usize>();
        if remove_count == 0 {
            self.status_msg = "No duplicate items marked for removal".into();
            return;
        }
        self.pending_action = Some(PendingAction::Dedup);
        self.mode = Mode::Confirm;
        self.status_msg = format!(
            "Execute dedup for {} files? [Enter/y=yes, n/Esc=no]",
            remove_count
        );
    }

    pub fn request_organize_execute(&mut self) {
        if self.organize_plans.is_empty() {
            self.status_msg = "No organize plans to execute".into();
            return;
        }
        self.pending_action = Some(PendingAction::Organize);
        self.mode = Mode::Confirm;
        self.status_msg = format!(
            "Execute {} organize plans? [Enter/y=yes, n/Esc=no]",
            self.organize_plans.len()
        );
    }

    pub fn confirm_pending_action(&mut self) {
        match self.pending_action.take() {
            Some(PendingAction::Rename) => {
                let renamer = Renamer::new(self.config.rename.clone());
                let dry_run = self.config.general.dry_run;
                let actions = renamer.execute(&self.rename_plans, dry_run);
                let renamed = actions
                    .iter()
                    .filter(|a| a.starts_with("[renamed]") || a.starts_with("[dry-run]"))
                    .count();
                self.mode = Mode::Normal;
                self.status_msg = format!("Rename executed: {} actions", renamed);
                if !dry_run {
                    self.scan();
                    self.status_msg = format!("Rename executed: {} actions", renamed);
                }
            }
            Some(PendingAction::Dedup) => {
                let deduplicator = Deduplicator::new(self.config.dedup.clone());
                let dry_run = self.config.general.dry_run;
                let rt = match crate::core::runtime::build() {
                    Ok(rt) => rt,
                    Err(err) => {
                        self.mode = Mode::Normal;
                        self.status_msg = err;
                        return;
                    }
                };
                let actions = rt
                    .block_on(deduplicator.execute(&self.dedup_groups, &self.items, dry_run))
                    .unwrap_or_else(|e| vec![format!("Error: {e}")]);
                let processed = actions
                    .iter()
                    .filter(|a| {
                        a.starts_with("[trash]")
                            || a.starts_with("[move]")
                            || a.starts_with("[report]")
                            || a.starts_with("[dry-run]")
                    })
                    .count();
                self.mode = Mode::Normal;
                self.status_msg = format!("Dedup executed: {} actions", processed);
                if !dry_run {
                    self.scan();
                    self.status_msg = format!("Dedup executed: {} actions", processed);
                }
            }
            Some(PendingAction::Organize) => {
                let organizer = Organizer::new(self.config.organize.clone());
                let dry_run = self.config.general.dry_run;
                let actions = organizer.execute(&self.organize_plans, dry_run);
                let processed = actions
                    .iter()
                    .filter(|a| {
                        a.starts_with("[move]")
                            || a.starts_with("[copy]")
                            || a.starts_with("[hardlink]")
                            || a.starts_with("[symlink]")
                            || a.starts_with("[nfo]")
                            || a.starts_with("[image]")
                            || a.starts_with("[dry-run]")
                    })
                    .count();
                self.mode = Mode::Normal;
                self.status_msg = format!("Organize executed: {} actions", processed);
                if !dry_run {
                    self.scan();
                    self.status_msg = format!("Organize executed: {} actions", processed);
                }
            }
            None => {
                self.mode = Mode::Normal;
                self.status_msg = "Nothing to confirm".into();
            }
        }
    }

    pub fn cancel_pending_action(&mut self) {
        self.pending_action = None;
        self.mode = Mode::Normal;
        self.status_msg = "Cancelled".into();
    }

    pub fn toggle_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Table => ViewMode::Tree,
            ViewMode::Tree => ViewMode::Table,
        };
        self.selected = 0;
        self.status_msg = format!("View: {:?}", self.view_mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::OrganizeMode;
    use crate::engine::deduplicator::{DuplicateGroup, DuplicateItem};
    use crate::engine::organizer::{OrganizeAction, OrganizePlan};
    use crate::models::media::{MediaItem, MediaType, RenamePlan};

    fn make_app() -> App {
        let mut config = AppConfig::default();
        config.general.dry_run = true;
        config.organize.mode = OrganizeMode::Archive;
        App::new(config, ".".into())
    }

    fn make_item(name: &str) -> MediaItem {
        MediaItem {
            id: 0,
            path: PathBuf::from(format!("/tmp/{name}")),
            file_size: 1024,
            media_type: MediaType::Movie,
            extension: "mkv".into(),
            parsed: None,
            quality: None,
            scraped: None,
            hash: None,
            rename_plan: None,
        }
    }

    #[test]
    fn test_request_rename_execute_enters_confirm_mode() {
        let mut app = make_app();
        app.rename_plans.push(RenamePlan {
            old_path: PathBuf::from("/tmp/old.mkv"),
            new_path: PathBuf::from("/tmp/new.mkv"),
            subtitle_plans: Vec::new(),
            directory_plans: Vec::new(),
        });

        app.request_rename_execute();

        assert_eq!(app.mode, Mode::Confirm);
        assert_eq!(app.pending_action, Some(PendingAction::Rename));
    }

    #[test]
    fn test_confirm_rename_execute_clears_pending_action() {
        let mut app = make_app();
        app.rename_plans.push(RenamePlan {
            old_path: PathBuf::from("/tmp/old.mkv"),
            new_path: PathBuf::from("/tmp/new.mkv"),
            subtitle_plans: Vec::new(),
            directory_plans: Vec::new(),
        });

        app.request_rename_execute();
        app.confirm_pending_action();

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.pending_action, None);
        assert!(app.status_msg.contains("Rename executed"));
    }

    #[test]
    fn test_request_dedup_execute_enters_confirm_mode() {
        let mut app = make_app();
        app.items.push(make_item("a.mkv"));
        app.items.push(make_item("b.mkv"));
        app.dedup_groups.push(DuplicateGroup {
            content_id: "hash:1".into(),
            items: vec![
                DuplicateItem {
                    index: 0,
                    quality_score: 10.0,
                    is_keep: true,
                },
                DuplicateItem {
                    index: 1,
                    quality_score: 5.0,
                    is_keep: false,
                },
            ],
        });

        app.request_dedup_execute();

        assert_eq!(app.mode, Mode::Confirm);
        assert_eq!(app.pending_action, Some(PendingAction::Dedup));
    }

    #[test]
    fn test_confirm_dedup_execute_clears_pending_action() {
        let mut app = make_app();
        app.items.push(make_item("a.mkv"));
        app.items.push(make_item("b.mkv"));
        app.dedup_groups.push(DuplicateGroup {
            content_id: "hash:1".into(),
            items: vec![
                DuplicateItem {
                    index: 0,
                    quality_score: 10.0,
                    is_keep: true,
                },
                DuplicateItem {
                    index: 1,
                    quality_score: 5.0,
                    is_keep: false,
                },
            ],
        });

        app.request_dedup_execute();
        app.confirm_pending_action();

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.pending_action, None);
        assert!(app.status_msg.contains("Dedup executed"));
    }

    #[test]
    fn test_request_organize_execute_enters_confirm_mode() {
        let mut app = make_app();
        app.organize_plans.push(OrganizePlan {
            source: PathBuf::from("/tmp/a.mkv"),
            target: PathBuf::from("/tmp/Movies/a.mkv"),
            action: OrganizeAction::Move,
            nfo_content: None,
            image_urls: Vec::new(),
        });

        app.request_organize_execute();

        assert_eq!(app.mode, Mode::Confirm);
        assert_eq!(app.pending_action, Some(PendingAction::Organize));
    }

    #[test]
    fn test_cancel_pending_action_resets_mode() {
        let mut app = make_app();
        app.rename_plans.push(RenamePlan {
            old_path: PathBuf::from("/tmp/old.mkv"),
            new_path: PathBuf::from("/tmp/new.mkv"),
            subtitle_plans: Vec::new(),
            directory_plans: Vec::new(),
        });

        app.request_rename_execute();
        app.cancel_pending_action();

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.pending_action, None);
        assert_eq!(app.status_msg, "Cancelled");
    }
}
