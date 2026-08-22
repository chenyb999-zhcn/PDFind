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
    └── DirTree.vue     # File tree panel (lazy-load, PDF-only filter)

src-tauri/src/          # Rust backend
├── lib.rs              # Tauri entry, command registration
├── commands.rs         # search_file, start_search, cancel_search, get_ocr_words
├── pdfx.rs             # pdfium text extraction + OCR integration
├── ocr.rs              # Windows OCR (WinRT, cfg(windows) only)
├── engine.rs           # Regex matcher (escape, whole-word, case)
├── tree.rs             # Directory tree listing (dirs + PDFs only)
├── walker.rs           # Recursive PDF collection with cancel support
├── state.rs            # SearchState: single-task guard + cancel flag
└── cache.rs            # OCR result cache (path + size + mtime key)
```

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

## Version Bumping

Update version in three places:
1. `package.json` → `"version"`
2. `src-tauri/Cargo.toml` → `version`
3. `src-tauri/tauri.conf.json` → `"version"`

Then run `cargo update -p pdfind` to sync Cargo.lock.

## Planned Features

See `todo.md` for P2P search roadmap (eD2k, DC++/ADC, BitTorrent index sites).
