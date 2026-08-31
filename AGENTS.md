# PDFind - Agent Guide

PDF 全文搜索桌面工具。Tauri 2 + Vue 3 + Rust。

## Commands

```bash
# Development (hot reload)
npm run tauri dev

# Type check + build frontend
npm run build

# Build release exe (requires pdfium.dll in src-tauri/binaries/)
npm run tauri build -- --no-bundle

# Rust check only
cargo check --manifest-path src-tauri/Cargo.toml
```

## Architecture

```
src/                    # Vue 3 frontend
├── App.vue             # Main UI: 顶部 tab (搜索/视频转PDF/知识库) + search toolbar, results, preview panel
└── components/
    ├── Preview.vue     # PDF preview with text layer + keyword highlighting
    ├── DirTree.vue     # File tree panel (lazy-load, PDF-only filter)
    ├── VideoToPdf.vue  # 视频转PDF: 转写/日志/整理配置/PDF生成 UI (含"存入知识库"勾选)
    └── KnowledgeBase.vue # 知识库: 文档列表 + RAG 问答聊天 + 检索 + embedding 模型下载

src-tauri/src/          # Rust backend
├── lib.rs              # Tauri entry, command registration (含 kb::commands)
├── commands.rs         # search_file, start_search, cancel_search, get_ocr_words
├── pdfx.rs             # pdfium text extraction + OCR integration
├── ocr.rs              # Windows OCR (WinRT, cfg(windows) only)
├── engine.rs           # Regex matcher (escape, whole-word, case)
├── tree.rs             # Directory tree listing (dirs + PDFs only)
├── walker.rs           # Recursive PDF collection with cancel support
├── state.rs            # SearchState: single-task guard + cancel flag
├── cache.rs            # OCR result cache (path + size + mtime key)
├── kb/                 # 知识库 RAG 模块 (本地 BGE embedding + SQLite 混合检索 + LLM 问答)
│   ├── db.rs           # SQLite (rusqlite bundled): documents/chunks + FTS5 trigram + 触发器
│   ├── chunking.rs     # ~400字分块 (视频按segments带时间戳 / PDF按页 / 纯文本按段)
│   ├── embed.rs        # embedding 模型下载 + 调 llama-funasr-cli --embed 批量编码
│   ├── ingest.rs       # 入库: 视频/PDF/文本 (同 kind+来源 覆盖), 写 doc+chunks+向量
│   ├── retrieve.rs     # 混合检索: 向量点积 + FTS bm25, RRF(K=60) 融合
│   └── commands.rs     # kb_overview/add_pdf/add_text/remove_doc/ask/search/download_embed_model
└── v2p/                # 视频转 PDF 模块 (FunASR 转写 + 在线整理 + PDF 生成)
    ├── models.rs       # 模型清单 + organizer_providers()（7家在线服务商预置模型列表）
    ├── commands.rs     # v2p_check_env/transcribe/generate_pdf(save_to_kb)/organizer 配置
    │                   #   + v2p_list_organizer_models（GET /models 动态拉取模型列表）
    ├── organizer.rs    # 在线整理配置 {app_config_dir}/organizer.json (Key 本地存储)
    ├── llm.rs          # 通用 OpenAI 兼容 chat (resolve_llm + llm_chat, v2p整理/kb问答共用)
    ├── llamacpp.rs     # llama-funasr-cli 子进程 (stdout=结果, stderr=日志 \x01LOG\x01 前缀; 另有 --embed 模式)
    └── pdf.rs          # PDF 渲染 (ffmpeg 场景取帧 + rusttype 中文字体)
```

## v2p 模块要点

- **转写 CLI**：`src-tauri/dev-models/bin-cuda/llama-funasr-cli.exe`（CUDA 版，捆绑 cublas/cudart DLL）/ `bin/`（CPU 版）。`dev-models/` 不进安装包，仅开发用；生产由 `v2p_download_model` 下载到 `app_config_dir/models/`
- **stdout/stderr 分流**：CLI stderr 由后端加 `\x01LOG\x01` 前缀 → `v2p:log` 事件；stdout 行 → 分段结果（`[start-end] text` 格式）→ `v2p:result` 事件（Segment{text,start,end}）
- **在线整理**：ureq POST `{base_url}/chat/completions`（OpenAI 兼容，Bearer，temp 0.6，300s 超时）。Cargo.toml 的 ureq 需 `features=["json"]`。失败自动回退按时间戳分章
- **模型下拉**：预置列表（models.rs）∪ 动态拉取（`v2p_list_organizer_models` 调 GET /models）∪ 手动输入兜底
- **PDF 中文字体**：rusttype 不支持 TTC 字体集合，优先 `C:/Windows/Fonts/Deng.ttf`，回退 simsun/simhei
- **事件**：`v2p:log` / `v2p:dl`（下载进度）/ `v2p:result`

## kb 模块要点 (知识库 RAG)

- **存储**：`{app_data_dir}/kb.sqlite`（rusqlite bundled，WAL）。`documents` + `chunks`（embedding BLOB 为 f32 LE 数组）+ FTS5 `trigram` 外部内容表（支持中文子串，查询需 ≥3 字符）+ 增删改触发器
- **embedding**：BGE-small-zh-v1.5（4层/512维/CLS pooling，q8_0 GGUF ~26MB），模型放 `model_dir()/bge-small-zh-v1.5/`，由 `kb_download_embed_model` 从 GitHub Release v0.4.0 下载；编码调 `llama-funasr-cli -m <gguf> --embed --input <txt>`（每行一条，stdout 每行 `{"i":N,"dim":D,"v":[...]}` L2 归一化）
- **入库**：视频=转写 segments 聚块（带时间戳，LLM 整理大纲存 doc.meta 展示）；PDF=pdfium 抽页按页分块；文本=按段。同 kind+source_path 重加 = 覆盖（级联删块）。模型未下载时 chunk 存 NULL 向量，检索自动退化为纯关键词
- **检索**：向量点积 top-30 + FTS bm25 top-30 → RRF(K=60) 融合 → top-8 喂 LLM
- **问答**：`kb_ask` 用 `v2p::llm::resolve_llm`（复用 organizer.json 的 7 家服务商 Key），system 提示要求 [n] 引用，回答附 sources（doc_title+时间戳/页码+snippet）
- **CLI --embed**：`funasr-cli` 新增模式（`cp.embeddings=true` + `llama_get_embeddings_seq`）；该 GGUF 的 `tokenizer.ggml.pre` 串对 WPM 类模型无效，llama.cpp 运行时内置 BERT 预分词（[CLS]...[SEP]）
- **事件**：`kb:log` / `kb:dl`（embedding 模型下载进度）

## Key Patterns

- **Event streaming**: Directory search uses Tauri events (`search:progress`, `search:result`, `search:done`) for non-blocking UI
- **Cancel mechanism**: `AtomicBool` in `SearchState`, checked per-file in walker loop
- **OCR trigger**: Pages with <300 chars AND large image (≥30% page area) get OCR'd
- **Text selection**: Invisible text layer overlay on canvas (`color: transparent`, `user-select: text`)
- **Width persistence**: Preview panel width stored in localStorage as ratio, not pixels

## Gotchas

- **pdfium.dll required**: Must be manually placed in `src-tauri/binaries/` (not in repo)
- **Windows-only**: OCR uses WinRT (`windows` crate with `Media_Ocr` feature)
- **crate-type = ["rlib"]**: Only rlib, no cdylib/staticlib (avoids msys2 ld link failures)
- **Frontend regex must match backend**: `kwRegex()` in Preview.vue mirrors Rust `engine.rs` escaping
- **Chinese UI**: All user-facing strings are in Chinese
- **CUDA 转写需 DLL 同目录**: `bin-cuda/llama-funasr-cli.exe` 依赖 cublas64_13.dll/cublasLt64_13.dll/cudart64_13.dll，必须与 exe 同目录（已捆绑），不要单独移动 exe
- **GLM /models 不含 flash 系列**: 智谱 GET /models 返回不含 glm-*-flash 模型，需靠预置列表/手动输入
- **豆包用 Endpoint ID**: 火山方舟 model 参数应填 `ep-xxx` Endpoint ID 或带日期后缀的模型 ID，推荐“获取列表”或手动输入

## Version Bumping

Update version in three places:
1. `package.json` → `"version"`
2. `src-tauri/Cargo.toml` → `version`
3. `src-tauri/tauri.conf.json` → `"version"`

Then run `cargo update -p pdfind` to sync Cargo.lock.

## Planned Features

See `todo.md` for P2P search roadmap (eD2k, DC++/ADC, BitTorrent index sites).
