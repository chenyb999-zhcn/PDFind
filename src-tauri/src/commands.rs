// Tauri 命令: 单文件搜索(同步) + 目录搜索(事件流) + 取消 + OCR 词框
use crate::cache::{self, OcrWordBox};
use crate::{engine::Matcher, pdfx, state::SearchState, walker};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

// 一条命中: 前文/命中串/后文 三段,避免跨语言字符偏移量错位
#[derive(Serialize, Clone)]
pub struct Hit {
    pub page: u32,
    pub pre: String,
    pub matched: String,
    pub post: String,
}

#[derive(Serialize, Clone)]
pub struct FileSearchResult {
    pub path: String,
    pub total_pages: u32,
    pub hits: Vec<Hit>,
}

#[derive(Serialize, Clone)]
pub struct ProgressEvent {
    pub scanned: usize,
    pub total: usize,
    pub matched: usize,
    pub current: String,
}

#[derive(Serialize, Clone)]
pub struct DoneEvent {
    pub cancelled: bool,
    pub scanned: usize,
    pub total: usize,
    pub matched: usize,
    pub skipped: usize,
    pub hits: usize,
}

// 逐页匹配; 同一行只报首条命中避免刷屏
fn match_pages(matcher: &Matcher, pages: &[String]) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    for (i, text) in pages.iter().enumerate() {
        let mut last_line: Option<(usize, usize)> = None;
        for m in matcher.re.find_iter(text) {
            let (start, end) = (m.start(), m.end());
            let ls = text[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let le = text[end..].find('\n').map(|p| end + p).unwrap_or(text.len());
            if last_line == Some((ls, le)) {
                continue;
            }
            last_line = Some((ls, le));
            let line = text[ls..le].trim_end_matches(['\r', '\n']);
            let rel = start - ls;
            let matched_len = line.len().min(rel + (end - start)) - rel;
            hits.push(Hit {
                page: i as u32 + 1,
                pre: line[..rel].to_string(),
                matched: line[rel..rel + matched_len].to_string(),
                post: line[rel + matched_len..].to_string(),
            });
        }
    }
    hits
}

#[tauri::command]
pub async fn search_file(
    app: AppHandle,
    state: State<'_, SearchState>,
    path: String,
    pattern: String,
    case_insensitive: bool,
    whole_word: bool,
    use_ocr: bool,
) -> Result<FileSearchResult, String> {
    // 单文件搜索也注册取消槽位, 支持中途取消
    let cancel = state.begin().ok_or("已有搜索在进行中")?;
    let result = (|| {
        let matcher = Matcher::new(&pattern, false, case_insensitive, whole_word)?;
        let pdfium = pdfx::get(&app)?;
        let outcome =
            pdfx::extract_pages_cached(&app, pdfium, Path::new(&path), Some(&cancel), use_ocr)?;
        let hits = match_pages(&matcher, &outcome.texts);
        Ok(FileSearchResult {
            path,
            total_pages: outcome.texts.len() as u32,
            hits,
        })
    })();
    state.end();
    result
}

// 目录搜索: 后台线程逐文件提取匹配,事件流式推送进度/结果/完成
fn run_dir_search(
    app: &AppHandle,
    root: &Path,
    matcher: Matcher,
    cancel: Arc<AtomicBool>,
    use_ocr: bool,
) {
    let files = walker::collect_pdfs(root, &cancel);
    let total = files.len();
    let (mut scanned, mut matched, mut skipped, mut hits_total) = (0usize, 0usize, 0usize, 0usize);

    let _ = app.emit(
        "search:progress",
        ProgressEvent {
            scanned: 0,
            total,
            matched: 0,
            current: root.display().to_string(),
        },
    );

    let pdfium = match pdfx::get(app) {
        Ok(p) => p,
        Err(e) => {
            let _ = app.emit("search:done", DoneEvent {
                cancelled: false, scanned: 0, total, matched: 0, skipped: 0, hits: 0,
            });
            let _ = app.emit("search:error", e);
            return;
        }
    };

    for f in &files {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        scanned += 1;
        let current = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let current_clone = current.clone();
        let _ = app.emit(
            "search:progress",
            ProgressEvent {
                scanned,
                total,
                matched,
                current: current_clone,
            },
        );
        match pdfx::extract_pages_cached(app, pdfium, f, Some(&cancel), use_ocr) {
            Ok(outcome) => {
                // OCR 进行过则更新进度提示
                if outcome.ocr_pages > 0 {
                    let _ = app.emit(
                        "search:progress",
                        ProgressEvent {
                            scanned,
                            total,
                            matched,
                            current: format!("{current} (OCR×{})", outcome.ocr_pages),
                        },
                    );
                }
                let hits = match_pages(&matcher, &outcome.texts);
                if !hits.is_empty() {
                    matched += 1;
                    hits_total += hits.len();
                    let _ = app.emit(
                        "search:result",
                        FileSearchResult {
                            path: f.display().to_string(),
                            total_pages: outcome.texts.len() as u32,
                            hits,
                        },
                    );
                }
            }
            Err(_) => {
                skipped += 1; // 加密/损坏/取消,计入跳过
            }
        }
    }

    let _ = app.emit(
        "search:done",
        DoneEvent {
            cancelled: cancel.load(Ordering::SeqCst),
            scanned,
            total,
            matched,
            skipped,
            hits: hits_total,
        },
    );
}

#[tauri::command]
pub async fn start_search(
    app: AppHandle,
    state: State<'_, SearchState>,
    path: String,
    pattern: String,
    case_insensitive: bool,
    whole_word: bool,
    use_ocr: bool,
) -> Result<(), String> {
    let matcher = Matcher::new(&pattern, false, case_insensitive, whole_word)?;
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err("路径不是目录".into());
    }
    let cancel = state.begin().ok_or("已有搜索在进行中")?;

    let handle = app.clone();
    std::thread::spawn(move || {
        run_dir_search(&handle, &p, matcher, cancel, use_ocr);
        handle.state::<SearchState>().end();
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_search(state: State<'_, SearchState>) -> bool {
    state.cancel()
}

// 预览层 OCR 词框: 优先缓存; 未缓存则现场提取整本后返回该页(供前端叠加红线)
#[tauri::command]
pub async fn get_ocr_words(
    app: AppHandle,
    path: String,
    page: u32,
) -> Result<Vec<OcrWordBox>, String> {
    let p = PathBuf::from(&path);
    let key = cache::key_for(&p);
    let lookup = |pages: &[crate::cache::OcrPageResult]| -> Vec<OcrWordBox> {
        pages
            .get(page.saturating_sub(1) as usize)
            .map(|pr| pr.words.clone())
            .unwrap_or_default()
    };
    if let Some(pages) = cache::get(&app, &key, &p) {
        return Ok(lookup(&pages));
    }
    let pdfium = pdfx::get(&app)?;
    pdfx::extract_pages_cached(&app, pdfium, &p, None, true)?;
    let pages = cache::get(&app, &key, &p).unwrap_or_default();
    Ok(lookup(&pages))
}
