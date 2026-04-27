# Medio — Agent 文档

> 媒体文件智能管理工具：AI 识别 · 重命名 · 去重 · 整理归类 · 刮削

---

## 一、项目定位

Medio 是一个**纯 CLI 工具**（带可选 TUI），用于管理本地硬盘上混乱的媒体文件。核心解决四个痛点：

1. **命名混乱** — 下载的文件名五花八门，压制组标签混杂，无法一眼识别内容
2. **识别困难** — 中文/番剧文件名极度混乱，纯正则无法可靠解析，需要 AI 辅助
3. **重复下载** — 同一资源多次下载，浪费空间
4. **目录无序** — 电影、剧集、音乐、小说混在一起，没有规范目录结构

**参考项目**：

- [tw93/Mole](https://github.com/tw93/mole) — CLI 设计风格、安全优先理念、dry-run 默认
- [shai102/media-renamer-ai](https://github.com/shai102/media-renamer-ai) — AI 识别三档模式、Bangumi 刮削、关键词过滤、media_suffix 保留、NFO/图片刮削

设计原则：**安全优先**（所有破坏性操作默认 dry-run）、**零外部依赖**（核心功能不依赖 ffmpeg/Python）、**AI 可选**（无 AI 也能用，有 AI 更准）、**单二进制分发**。

---

## 二、业务功能清单

> 标记 ✅ 已确认，❌ 暂不需要，🔄 待讨论。标注 🔵 来自 media-renamer-ai 启发。

### 2.1 扫描与识别

| #   | 功能               | 描述                                                                                                                  | 状态       |
| --- | ------------------ | --------------------------------------------------------------------------------------------------------------------- | ---------- |
| S1  | 目录扫描           | 递归扫描指定目录，识别所有媒体文件（视频/音频/文档/图片/STRM）                                                        | ✅         |
| S2  | 类型检测           | 根据扩展名判断媒体类型（电影/剧集/音乐/小说/其他）                                                                    | ✅         |
| S3  | 文件名解析（辅助） | 正则+启发式提取标题/年份/季集等，作为刮削搜索的初始线索；因用户文件名混乱，不作为主要识别手段，重命名以刮削元数据为准 | ✅（辅助） |
| S4  | 排除规则           | 支持排除指定目录/文件（.git、样本文件、小于指定大小）                                                                 | ✅         |
| S5  | 🔵 关键词过滤      | 识别前自动剔除干扰关键词（压制组名、片源标签等），提升匹配率                                                          | ✅         |
| S6  | 🔵 父目录信息推断  | 文件名缺少年份时向上查找父目录年份；Season 从目录名推断                                                               | ✅         |
| S7  | 🔵 STRM 文件支持   | 支持 .strm 流媒体链接文件的识别和重命名                                                                               | ✅         |

### 2.2 AI 辅助识别 🔵

> 当文件名混乱导致 S3 正则解析无法可靠提取信息时，AI 自动介入辅助识别。

**与 S3 的协作流程**：

```
S3 正则解析 → 提取到有效标题？
  ├─ 是 → 用提取结果搜索刮削数据库
  └─ 否 → AI 介入解读文件名 → 提取标题 → 搜索刮削数据库
刮削数据库有结果？
  ├─ 是 → 使用刮削元数据重命名
  └─ 否 → AI 重提标题建议 → 二次搜索刮削数据库
```

| #   | 功能                  | 描述                                                                                   | 状态 |
| --- | --------------------- | -------------------------------------------------------------------------------------- | ---- |
| A1  | AI 智能回退识别       | 当 S3 正则解析失败或刮削无结果时，AI 自动介入解读文件名/重提标题，无需用户手动切换模式 | ✅   |
| A2  | 多 AI 后端            | 支持 DeepSeek、Cloudflare Workers AI（免费）、及任何 OpenAI 兼容接口，配置即用         | ✅   |
| A4  | 🔵 Embedding 候选重排 | 用 Embedding 模型对搜索候选做语义重排，提升匹配准确率                                  | ✅   |

### 2.3 元数据刮削

| #      | 功能             | 描述                                                                           | 状态 |
| ------ | ---------------- | ------------------------------------------------------------------------------ | ---- |
| M1     | TMDB 刮削        | TMDB API 获取电影/剧集元数据（标题、年份、评分、海报等），含中文数据           | ✅   |
| ~~M2~~ | ~~Bangumi 刮削~~ | ~~Bangumi API 获取番组/动漫元数据~~                                            | ❌   |
| ~~M3~~ | ~~豆瓣刮削~~     | ~~豆瓣获取中文元数据（非官方 API，维护成本高）~~                               | ❌   |
| M4     | MusicBrainz 刮削 | MusicBrainz API 获取音乐元数据（艺术家、专辑、曲目号）                         | ✅   |
| M5     | 本地 NFO 读取    | 读取已有 .nfo 文件，避免重复刮削                                               | ✅   |
| M6     | 刮削缓存         | 已刮削元数据本地缓存（sled），避免重复请求 API                                 | ✅   |
| M7     | 多源 Fallback    | 刮削链：本地 NFO → TMDB（影视）/ MusicBrainz（音乐）→ AI 辅助重提 → 文件名推断 | ✅   |
| M8     | 中文标题优先     | 可配置优先使用中文标题进行重命名                                               | ✅   |
| M9     | 🔵 NFO 文件生成  | 批量生成 .nfo 元数据文件（Kodi/Plex/Jellyfin 兼容）                            | ✅   |
| M10    | 🔵 图片刮削      | 下载 poster（海报）、fanart（背景）、still（剧照）到媒体目录                   | ✅   |

### 2.4 媒体质量分析

| #   | 功能                 | 描述                                                                          | 状态 |
| --- | -------------------- | ----------------------------------------------------------------------------- | ---- |
| Q1  | 原生媒体探测         | 纯 Rust 解析 MKV/MP4/FLAC/MP3 头信息，获取分辨率/编码/码率/时长               | ✅   |
| Q2  | ffprobe 增强         | 可选 ffprobe 获取更完整媒体信息                                               | ✅   |
| Q3  | 质量评分             | 综合分辨率(40%)、编码(30%)、码率(20%)、音频(10%) 计算质量分                   | ✅   |
| Q4  | 🔵 media_suffix 提取 | 从原文件名提取质量后缀（如 `2160p.WEB-DL.H.265.AAC-ColorTV`），可保留到重命名 | ✅   |
| Q5  | 来源识别             | 从文件名识别来源标签：BluRay/WEB-DL/HDTV/CAM/Remux 等                         | ✅   |

### 2.5 去重

> 核心场景：同一剧集/电影存在多个质量版本（如 80GB 4K vs 20GB 1080p），自动识别并保留高质量版本、移除低质量版本。

| #   | 功能         | 描述                                                                                                  | 状态 |
| --- | ------------ | ----------------------------------------------------------------------------------------------------- | ---- |
| D1  | 精确去重     | xxHash 完全匹配 → 完全相同的重复文件                                                                  | ✅   |
| D2  | 渐进式哈希   | 按文件大小分组 → 前 64KB 哈希 → 完整哈希，避免全量计算                                                | ✅   |
| D3  | 质量版本去重 | 同一内容不同质量版本（如 80GB 4K Remux vs 20GB 1080p WEB-DL），按质量评分(Q3)保留最优，移除低质量版本 | ✅   |
| D4  | 同内容识别   | 通过刮削元数据匹配：同标题 + 同季集号 + 相近时长 → 判定为同一内容的不同质量版本                       | ✅   |
| D5  | 保留策略     | 可配置：最高质量（默认）/ 最新文件 / 最大文件 / 手动选择                                              | ✅   |
| D6  | 重复文件处理 | 删除（移废纸篓）/ 移动到指定目录 / 仅报告不操作                                                       | ✅   |

### 2.6 重命名

| #   | 功能                 | 描述                                                                                                             | 状态 |
| --- | -------------------- | ---------------------------------------------------------------------------------------------------------------- | ---- |
| R1  | 模板化重命名         | 可配置模板，支持变量：title, year, season, episode, resolution, codec, ext 等                                    | ✅   |
| R2  | 🔵 高级模板语法      | Tera (Jinja2) 条件模板：`{% if media_suffix %} - {{ media_suffix }}{% endif %}`                                  | ✅   |
| R3  | 🔵 旧占位符兼容      | 同时支持 `{title} - S{s:02d}E{e:02d}{ext}`，自动转换                                                             | ✅   |
| R4  | 🔵 media_suffix 变量 | 模板可用 `{{ media_suffix }}`，未显式写入时可选自动追加                                                          | ✅   |
| R5  | 电影模板             | `{{ title }}{% if year %} ({{ year }}){% endif %}{% if media_suffix %} - {{ media_suffix }}{% endif %}{{ ext }}` | ✅   |
| R6  | 剧集模板             | `{{ title }} - S{{ season }}E{{ episode }}{% if ep_name %} - {{ ep_name }}{% endif %}{{ ext }}`                  | ✅   |
| R7  | 音乐模板             | `{{ artist }}/{{ album }} ({{ year }})/{{ track }} - {{ title }}{{ ext }}`                                       | ✅   |
| R8  | 小说模板             | `{{ author }} - {{ title }}{{ ext }}`                                                                            | ✅   |
| R9  | 冲突处理             | 目标已存在：跳过 / 追加后缀 / 覆盖（需确认）                                                                     | ✅   |
| R10 | 预览模式             | 默认 dry-run，先展示前后对比，确认后执行                                                                         | ✅   |
| R11 | 字幕关联重命名       | 重命名视频时自动关联重命名字幕（.srt/.ass/.ssa）                                                                 | ✅   |
| R12 | 🔵 季偏移调整        | 支持季号偏移（如 S02E01 实为 S01E13+），番剧常见需求                                                             | ✅   |
| R13 | 🔵 非法字符清理      | 模板渲染后自动清理非法路径字符、空括号、多余连接符                                                               | ✅   |

### 2.7 整理归类

> 核心场景：根目录下按媒体类型建立归属子目录（Movies/TV Shows/Music/Books），将文件移动到对应目录。

| #   | 功能                      | 描述                                                                                | 状态 |
| --- | ------------------------- | ----------------------------------------------------------------------------------- | ---- |
| O1  | 🔵 原地重命名             | 只在原目录改名，不调整目录结构                                                      | ✅   |
| O2  | 🔵 归档移动               | 按配置的根目录生成分类子目录（Movies/TV Shows/Music/Books）并移动文件到对应归属目录 | ✅   |
| O3  | 🔵 原地整理               | 以当前父目录为根，就地建立标准媒体库分类结构（Jellyfin/Emby/Kodi 兼容）             | ✅   |
| O4  | 可配置根目录              | 归档移动的目标根目录可配置，默认在根目录下按类型建立子目录                          | ✅   |
| O5  | 移动/复制/硬链接/符号链接 | 四种文件操作模式可选                                                                | ✅   |
| O6  | 🔵 媒体服务器兼容         | 目录结构兼容 Plex/Jellyfin/Emby/Kodi 识别规则                                       | ✅   |
| O7  | 🔵 空目录清理             | 整理后自动删除空目录（在整理根目录前停止）                                          | ✅   |
| O8  | 🔵 整理联动刮削           | 整理完成后自动写入 NFO + 下载图片                                                   | ✅   |

### 2.8 TUI 交互界面

| #   | 功能            | 描述                                           | 状态 |
| --- | --------------- | ---------------------------------------------- | ---- |
| T1  | 🔵 分组树视图   | 添加路径 → Season/子目录 → 文件 的树形结构浏览 | ✅   |
| T2  | 交互式扫描      | TUI 中浏览扫描结果，查看文件详情               | ✅   |
| T3  | 去重交互        | TUI 中查看重复组并对当前 dedup 计划做整批确认执行 | ✅   |
| T4  | 重命名预览      | TUI 中预览重命名前后对比，并对当前 rename 计划做整批确认执行 | ✅   |
| T5  | 🔵 手动匹配     | 右键手动精准匹配、候选选择、批量锁定           | ❌   |
| T6  | Vim 键绑定      | 支持 h/j/k/l 导航（参考 Mole）                 | ✅   |
| T7  | 进度展示        | 实时显示扫描/哈希/刮削进度                     | ❌   |
| T8  | 🔵 作用域批处理 | 当前支持按 tab 对整批 dedup/rename/organize 计划确认执行 | ✅   |

### 2.9 通用功能

| #   | 功能            | 描述                                        | 状态 |
| --- | --------------- | ------------------------------------------- | ---- |
| G1  | dry-run 默认    | 所有破坏性操作默认只预览不执行              | ✅   |
| G2  | 操作日志        | 所有文件操作记录到日志文件，可回溯          | ✅   |
| G3  | JSON 输出       | 支持 --json 标志，输出机器可读结果          | ✅   |
| G4  | 配置文件        | ~/.config/medio/config.toml，所有参数可配置 | ✅   |
| G5  | 并行处理        | 文件扫描、哈希计算、API 请求均并行执行      | ✅   |
| G6  | 🔵 并发参数可配 | 识别预览、数据库匹配、AI 请求的并发数可配置 | ✅   |

---

## 三、需要确认的业务问题

### 3.1 媒体类型范围 ✅ 已确认

最终支持 4+1 种类型：

- **电影** (Movie) — .mkv, .mp4, .avi, .wmv 等
- **剧集** (TV Show) — 含番剧/动漫，同上扩展名，通过季/集信息区分，统一走 TMDB 刮削链
- **音乐** (Music) — .mp3, .flac, .wav, .ogg, .m4a 等，走 MusicBrainz 刮削
- **小说/文档** (Novel/Book) — .epub, .pdf, .txt, .mobi 等，需要刮削（OpenLibrary API 等）
- **STRM** (.strm) — Jellyfin/Emby 流媒体链接文件

**已确认**：

- ❌ 不支持漫画/图片
- ❌ 不支持有声书
- ✅ 小说需要刮削
- ✅ 番剧归入剧集，走 TMDB 刮削链

### 3.2 AI 识别模式 ✅ 已确认

**已确认**：

- ✅ AI 作为**辅助识别**：正则解析失败或刮削无结果时自动介入，非强制模式
- ✅ 优先级：**Cloudflare Workers AI（免费）→ DeepSeek API（性价比高）→ 自定义 OpenAI 兼容**
- ✅ Embedding 重排：按业务需要作为合适的功能支持，当候选较多且匹配不确定时自动启用

### 3.3 刮削 API 选择 ✅ 已确认

- **TMDB**：用户自行申请 API Key（免费），覆盖全球影视含中文，数据最全
- **MusicBrainz**：免费开放 API，音乐元数据最权威
- **OpenLibrary**：免费开放 API，小说/书籍元数据

**已确认**：

- ✅ 接受 TMDB 需用户申请 API Key
- ❌ 移除 Bangumi（番剧归入剧集走 TMDB）
- ❌ 移除豆瓣刮削（维护成本高，不稳定）

**刮削链**：

- 影视：本地 NFO → TMDB → AI 辅助重提 → 文件名推断
- 音乐：本地 NFO → MusicBrainz → AI 辅助重提 → 文件名推断
- 小说：本地 NFO → OpenLibrary → AI 辅助重提 → 文件名推断

### 3.4 去重策略细节 ✅ 已确认

**已确认**：

- ✅ 重复文件优先**移到废纸篓**（更安全），可配置改为永久删除
- ✅ 不同版本去重逻辑：导演剪辑版 vs 正式版 — 若内容相同则视为重复，若剧情内容不同则不视为重复
- ✅ 感知哈希（perceptual hash）：支持则用，不支持则通过综合分析（元数据+时长+质量评分）判断

### 3.5 重命名语言偏好 ✅ 已确认

**已确认**：

- ✅ 优先使用**中文标题**重命名（可配置切换为英文/双语）
- ✅ media_suffix **默认保留**，重命名后自动追加质量后缀到模板指定位置
- ✅ 按模板完整命名，所有可用信息（标题、年份、季集、media_suffix 等）均写入

### 3.6 整理归类的深度 ✅ 已确认

**已确认**：

- ✅ 三种整理模式都需要（原地重命名/归档移动/原地整理）
- ✅ 自动创建**媒体服务器兼容**的目录结构（Plex/Jellyfin/Emby）
- ✅ 整理时自动下载**海报/封面图**（poster/fanart）
- ✅ 生成 **NFO 文件**（供 Kodi/Plex 读取）
- ✅ 整理联动刮削

### 3.7 TUI 优先级 ✅ 已确认

**已确认**：

- ✅ **CLI 优先**，TUI 作为可选增强功能
- ✅ 不做 TUI 时，CLI 需要**交互式确认**（逐个 y/n 或批量确认）
- ✅ TUI 核心交互方式为**分组树视图**（路径 → Season/子目录 → 文件），因为媒体文件天然按此层级组织，且作用域批操作需要树形导航

---

## 四、技术架构（已确定）

| 层面       | 选型                                         | 说明                          |
| ---------- | -------------------------------------------- | ----------------------------- |
| 语言       | **Rust** (edition 2024)                      | 性能 + 单二进制               |
| CLI        | clap v4 (derive)                             | 编译时生成                    |
| TUI        | ratatui + crossterm                          | 成熟 Rust TUI                 |
| 文件扫描   | ignore (ripgrep 内核)                        | 极快                          |
| 哈希       | twox-hash (xxHash)                           | 比 MD5 快 10x+                |
| 并行       | rayon (CPU) + tokio (IO)                     | 双 runtime                    |
| HTTP       | reqwest (async)                              | AI + API 刮削                 |
| 序列化     | serde + serde_json                           | 零拷贝                        |
| 缓存       | sled (纯 Rust KV)                            | 零 C 依赖                     |
| 媒体探测   | symphonia + mp4parse (原生) / ffprobe (可选) | 双后端                        |
| 模板       | tera                                         | Jinja2 风格，天然支持条件语法 |
| AI 客户端  | reqwest + serde                              | OpenAI 兼容 / Ollama          |
| 配置       | toml + dirs                                  | TOML 格式                     |
| 日志       | tracing                                      | 结构化日志                    |
| 进度       | indicatif                                    | 进度条                        |
| 二进制目标 | ~10MB (LTO + strip)                          | 单文件分发                    |

---

## 五、项目结构

```
medio/
├── Cargo.toml
├── src/
│   ├── main.rs                  # 入口: clap dispatch
│   │
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── args.rs              # clap derive 定义
│   │   └── commands/
│   │       ├── mod.rs
│   │       ├── scan.rs          # medio scan
│   │       ├── scrape.rs        # medio scrape
│   │       ├── rename.rs        # medio rename
│   │       ├── dedup.rs         # medio dedup
│   │       ├── organize.rs      # medio organize
│   │       └── tui.rs           # medio tui
│   │
│   ├── core/
│   │   ├── mod.rs
│   │   ├── scanner.rs           # 目录遍历 (ignore crate)
│   │   ├── identifier.rs        # 文件名智能解析 (正则+启发式)
│   │   ├── hasher.rs            # 渐进式哈希 (xxhash + rayon)
│   │   ├── keyword_filter.rs    # 🔵 关键词过滤 (剔除干扰词)
│   │   ├── context_infer.rs     # 🔵 父目录年份/Season 推断
│   │   ├── config.rs            # ~/.config/medio/config.toml
│   │   └── types.rs             # MediaType, QualityScore 等类型
│   │
│   ├── ai/                      # 🔵 AI 辅助识别模块
│   │   ├── mod.rs               # AI 智能回退调度 (与 S3 联动)
│   │   ├── openai_compat.rs     # DeepSeek / Cloudflare Workers AI / OpenAI 兼容
│   │   └── embedding.rs         # Embedding 候选重排
│   │
│   ├── media/
│   │   ├── mod.rs
│   │   ├── probe.rs             # MediaProbe trait
│   │   ├── native_probe.rs      # Rust 原生解析 (symphonia/mp4parse)
│   │   ├── ffprobe.rs           # 可选 ffprobe 后端
│   │   └── suffix.rs            # 🔵 media_suffix 提取
│   │
│   ├── scraper/
│   │   ├── mod.rs               # Scraper trait + fallback chain
│   │   ├── tmdb.rs              # TMDB API v3
│   │   ├── musicbrainz.rs       # MusicBrainz API
│   │   ├── openlibrary.rs       # OpenLibrary API (小说)
│   │   ├── local.rs             # 本地 NFO 读取
│   │   └── image_scraper.rs     # 🔵 poster/fanart/still 下载
│   │
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── renamer.rs           # 模板渲染 + 文件重命名
│   │   ├── deduplicator.rs      # 渐进式去重 + 质量排序
│   │   ├── organizer.rs         # 三模式整理 (原地/归档/就地)
│   │   └── nfo_writer.rs       # 🔵 NFO 文件生成
│   │
│   ├── db/
│   │   ├── mod.rs
│   │   └── cache.rs             # sled KV 封装
│   │
│   ├── tui/
│   │   ├── mod.rs
│   │   ├── app.rs               # ratatui App 状态机
│   │   ├── ui.rs                # UI 渲染
│   │   ├── event.rs             # crossterm 事件
│   │   └── tree_view.rs         # 🔵 分组树视图
│   │
│   └── models/
│       ├── mod.rs
│       └── media.rs             # MediaItem, ParsedInfo, QualityInfo, ScrapeResult
│
└── tests/
```

---

## 六、命令设计

```bash
medio scan <path>              # 扫描并识别媒体文件
medio scrape <path>            # 刮削元数据 (NFO + 图片)
medio rename <path>            # 原地重命名（默认 --dry-run）
medio dedup <path>             # 去重（默认 --dry-run）
medio organize <path>          # 整理归类（默认 --dry-run）
medio analyze <path>           # 分析单个文件详情
medio tui                      # 交互式 TUI
medio config                   # 打开/编辑配置文件
medio --version
medio --help

# 通用标志
--dry-run                      # 仅预览不执行（默认开启）
--confirm                      # 逐个确认
--json                         # JSON 输出
--debug                        # 详细日志
--probe <native|ffprobe>       # 媒体探测后端
--no-ai                        # 🔵 禁用 AI 辅助（仅用正则+刮削）
--concurrency <n>              # 🔵 并发数

# organize 子命令标志
--mode <rename|archive|local>  # 🔵 整理模式: 原地重命名/归档移动/原地整理
--with-nfo                     # 🔵 整理时生成 NFO
--with-images                  # 🔵 整理时下载海报/剧照
--link <none|hard|sym>         # 链接模式: 移动/硬链接/符号链接
```

---

## 七、配置文件设计

```toml
# ~/.config/medio/config.toml

[general]
dry_run = true
confirm = true
log_level = "info"

[scan]
max_depth = 10
follow_symlinks = false
exclude_dirs = [".git", ".DS_Store", "Thumbs.db", "__MACOSX"]
min_file_size = "1MB"                    # 忽略过小文件
keyword_filter = []                       # 🔵 剔除关键词列表，如 ["字幕侠", "ColorTV", "WEBDL"]

[ai]                                       # 🔵 AI 识别配置（正则失败时自动回退）
enabled = true                              # 设为 false 禁用 AI（仅用正则+刮削）
provider = "deepseek"                       # deepseek / cloudflare / custom
# DeepSeek（推荐，性价比高）
deepseek_url = "https://api.deepseek.com/v1"
deepseek_key = ""
deepseek_model = "deepseek-chat"
# Cloudflare Workers AI（免费额度）
cf_url = "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai"
cf_api_token = ""
cf_model = "@cf/meta/llama-3.1-8b-instruct"
# 自定义 OpenAI 兼容接口
custom_url = ""
custom_key = ""
custom_model = ""
# Embedding 候选重排
embedding_provider = "deepseek"             # deepseek / cloudflare / custom
embedding_model = ""                        # 留空则不启用重排
concurrency = 3                            # AI 请求并发数

[api]
tmdb_key = ""                              # TMDB API Key
musicbrainz_user_agent = ""                 # MusicBrainz 要求设置 UA

[scrape]
fallback_chain = ["local", "tmdb", "musicbrainz", "openlibrary", "ai", "guess"]
chinese_title_priority = true              # 中文标题优先
with_nfo = false                           # 🔵 刮削时生成 NFO
with_images = false                        # 🔵 刮削时下载图片

[dedup]
hash_algorithm = "xxhash"
keep_strategy = "highest_quality"          # highest_quality / newest / largest
duplicate_action = "trash"                 # trash / move / report
move_target = ""                           # duplicate_action=move 时的目标目录

[rename]
movie_template = '{{ title }}{% if year %} ({{ year }}){% endif %}{% if media_suffix %} - {{ media_suffix }}{% endif %}{{ ext }}'
tv_template = '{{ title }} - S{{ season }}E{{ episode }}{% if ep_name %} - {{ ep_name }}{% endif %}{{ ext }}'
music_template = '{{ artist }}/{{ album }} ({{ year }})/{{ track }} - {{ title }}{{ ext }}'
novel_template = '{{ author }} - {{ title }}{{ ext }}'
preserve_media_suffix = true               # 🔵 模板未写 media_suffix 时自动追加
season_offset = 0                          # 🔵 季偏移
rename_subtitles = true                    # 字幕关联重命名

[organize]
mode = "archive"                           # 🔵 rename / archive / local
root = ""                                  # 归档移动目标根目录
link_mode = "none"                         # none / hard / sym
with_nfo = false                           # 🔵 整理联动 NFO
with_images = false                        # 🔵 整理联动图片
cleanup_empty_dirs = true                  # 🔵 空目录清理

[quality]
resolution_weight = 0.4
codec_weight = 0.3
bitrate_weight = 0.2
audio_weight = 0.1

[cache]
path = ""                                  # 默认 ~/.config/medio/cache.sled
ttl_days = 90                              # 启动时按 TTL 清理带时间戳的缓存记录
```

---

## 八、开发路线

| 阶段   | 内容                                                            | 交付物                  |
| ------ | --------------------------------------------------------------- | ----------------------- |
| **P0** | 项目骨架 + scan + 文件名解析 + 关键词过滤 + 父目录推断 + config | `medio scan` 可运行     |
| **P1** | 哈希去重 + 质量分析 + media_suffix + dedup                      | `medio dedup` 可运行    |
| **P2** | TMDB + MusicBrainz + OpenLibrary 刮削 + 高级模板 + rename       | `medio rename` 可运行   |
| **P3** | AI 智能回退 (DeepSeek/Cloudflare) + Embedding 重排              | AI 识别可用             |
| **P4** | 三模式整理 + NFO 生成 + 图片刮削 + organize                     | `medio organize` 可运行 |
| **P5** | TUI (分组树视图 + 手动匹配 + 作用域批处理)                      | `medio tui` 可运行      |
| **P6** | 打包发布 (brew/cargo) + 完善文档                                | 完整可用版本            |

---

## 九、确认方式

请逐项确认或调整第三节的业务问题，我会据此更新本文档并开始实现。标记方式：

- ✅ 确认需要
- ❌ 不需要
- 🔄 需要讨论/修改
- ➕ 新增需求
