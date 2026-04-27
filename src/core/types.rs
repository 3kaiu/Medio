use serde::{Deserialize, Serialize};

/// 去重保留策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeepStrategy {
    HighestQuality,
    Newest,
    Largest,
    Manual,
}

/// 重复文件处理方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DupAction {
    Trash,
    Move,
    Report,
}

/// 整理模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganizeMode {
    Rename,
    Archive,
    Local,
}

/// 文件操作模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMode {
    None,
    Hard,
    Sym,
}

/// AI 后端选择
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiProvider {
    DeepSeek,
    Cloudflare,
    Custom,
}
