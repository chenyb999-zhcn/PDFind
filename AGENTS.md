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
├── App.vue             # Main UI: search toolbar, results, preview panel
└── components/
    ├── Preview.vue     # PDF preview with text layer + keyword highlighting
    ├── DirTree.vue     # File tree panel (lazy-load, PDF-only filter)
    └── VideoToPdf.vue  # 视频转PDF: 转写/日志/整理配置/PDF生成 UI

src-tauri/src/          # Rust backend
├── lib.rs              # Tauri entry, command registration
├── commands.rs         # search_file, start_search, cancel_search, get_ocr_words
├── pdfx.rs             # pdfium text extraction + OCR integration
├── ocr.rs              # Windows OCR (WinRT, cfg(windows) only)
├── engine.rs           # Regex matcher (escape, whole-word, case)
├── tree.rs             # Directory tree listing (dirs + PDFs only)
├── walker.rs           # Recursive PDF collection with cancel support
├── state.rs            # SearchState: single-task guard + cancel flag
├── cache.rs            # OCR result cache (path + size + mtime key)
└── v2p/                # 视频转 PDF 模块 (FunASR 转写 + 在线整理 + PDF 生成)
    ├── models.rs       # 模型清单 + organizer_providers()（6家在线服务商预置模型列表）
    ├── commands.rs     # v2p_check_env/transcribe/generate_pdf/organizer 配置
    │                   #   + v2p_list_organizer_models（GET /models 动态拉取模型列表）
    ├── organizer.rs    # 在线整理配置 {app_config_dir}/organizer.json (Key 本地存储)
    ├── llamacpp.rs     # llama-funasr-cli 子进程 (stdout=结果, stderr=日志 \x01LOG\x01 前缀)
    └── genpdf.rs       # PDF 渲染 (ffmpeg 场景取帧 + rusttype 中文字体)
```

## v2p 模块要点

- **转写 CLI**：`src-tauri/dev-models/bin-cuda/llama-funasr-cli.exe`（CUDA 版，捆绑 cublas/cudart DLL）/ `bin/`（CPU 版）。`dev-models/` 不进安装包，仅开发用；生产由 `v2p_download_model` 下载到 `app_config_dir/models/`
- **stdout/stderr 分流**：CLI stderr 由后端加 `\x01LOG\x01` 前缀 → `v2p:log` 事件；stdout 行 → 分段结果（`[start-end] text` 格式）→ `v2p:result` 事件（Segment{text,start,end}）
- **在线整理**：ureq POST `{base_url}/chat/completions`（OpenAI 兼容，Bearer，temp 0.6，300s 超时）。Cargo.toml 的 ureq 需 `features=["json"]`。失败自动回退按时间戳分章
- **模型下拉**：预置列表（models.rs）∪ 动态拉取（`v2p_list_organizer_models` 调 GET /models）∪ 手动输入兜底
- **PDF 中文字体**：rusttype 不支持 TTC 字体集合，优先 `C:/Windows/Fonts/Deng.ttf`，回退 simsun/simhei
- **事件**：`v2p:log` / `v2p:dl`（下载进度）/ `v2p:result`

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
