use crate::core::config::AppConfig;
use crate::core::context_infer::ContextInfer;
use crate::core::identifier::Identifier;
use crate::core::keyword_filter::KeywordFilter;
use crate::core::scanner::Scanner;
use crate::models::media::MediaItem;

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
    #[allow(dead_code)]
    Confirm,
}

pub struct App {
    pub config: AppConfig,
    pub items: Vec<MediaItem>,
    pub tab: Tab,
    pub mode: Mode,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search_query: String,
    pub status_msg: String,
    pub should_quit: bool,
    pub path: String,
}

impl App {
    pub fn new(config: AppConfig, path: String) -> Self {
        Self {
            config,
            items: Vec::new(),
            tab: Tab::Scan,
            mode: Mode::Normal,
            selected: 0,
            scroll_offset: 0,
            search_query: String::new(),
            status_msg: "Press 's' to scan, Tab to switch views, q to quit".into(),
            should_quit: false,
            path,
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
            if let Some(parsed) = &item.parsed {
                let parent_dirs = collect_parent_dirs(&item.path, 3);
                let inferred = ContextInfer::infer(parsed, &parent_dirs);
                item.parsed = Some(inferred);
            }
        }

        self.selected = 0;
        self.scroll_offset = 0;
        self.status_msg = format!("Scanned {} files", self.items.len());
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
        let len = self.items.len();
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
        let len = self.items.len();
        if len > 0 {
            self.selected = len - 1;
            self.adjust_scroll();
        }
    }

    pub fn page_down(&mut self, page_size: usize) {
        let len = self.items.len();
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
        if self.search_query.is_empty() {
            self.items.iter().enumerate().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.items.iter().enumerate()
                .filter(|(_, item)| {
                    let name = item.path.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
                    name.contains(&q)
                        || item.parsed.as_ref().map(|p| p.raw_title.to_lowercase().contains(&q)).unwrap_or(false)
                        || item.scraped.as_ref().map(|s| s.title.to_lowercase().contains(&q)).unwrap_or(false)
                })
                .collect()
        }
    }
}

fn collect_parent_dirs(path: &std::path::Path, max: usize) -> Vec<&std::path::Path> {
    let mut dirs = Vec::new();
    let mut current = path.parent();
    while let Some(dir) = current {
        if dirs.len() >= max { break; }
        dirs.push(dir);
        current = dir.parent();
    }
    dirs
}
