// pdfium 动态库加载与文本提取 + OCR 集成
use crate::cache::{self, OcrPageResult};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::Manager;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

// pdfium 动态库文件名按平台区分
#[cfg(windows)]
const PDFIUM_LIB: &str = "pdfium.dll";
#[cfg(target_os = "macos")]
const PDFIUM_LIB: &str = "libpdfium.dylib";
#[cfg(target_os = "linux")]
const PDFIUM_LIB: &str = "libpdfium.so";

// 按优先级尝试的候选路径:打包资源目录 > exe 同目录 > 开发时 src-tauri/binaries
fn dll_candidates(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(rd) = resource_dir {
        v.push(rd.join(PDFIUM_LIB));
        v.push(rd.join("binaries").join(PDFIUM_LIB));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join(PDFIUM_LIB));
        }
    }
    v.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(PDFIUM_LIB),
    );
    v
}

// 获取全局 Pdfium 实例(懒加载,进程内只绑定一次)
pub fn get(app: &tauri::AppHandle) -> Result<&'static Pdfium, String> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let rd = app.path().resource_dir().ok();
    let mut last_err = String::new();
    for cand in dll_candidates(rd.as_deref()) {
        match Pdfium::bind_to_library(cand.as_path()) {
            Ok(bindings) => {
                let _ = PDFIUM.set(Pdfium::new(bindings));
                return Ok(PDFIUM.get().unwrap());
            }
            Err(e) => last_err = format!("{}: {}", cand.display(), e),
        }
    }
    Err(format!("无法加载 pdfium.dll ({last_err})"))
}

// 逐页提取文本,每页一个 String(含页码定位)
pub fn extract_pages(
    pdfium: &Pdfium,
    path: &Path,
    password: Option<&str>,
) -> Result<Vec<String>, PdfiumError> {
    let doc = pdfium.load_pdf_from_file(path, password)?;
    let mut out = Vec::new();
    for page in doc.pages().iter() {
        let text = page.text()?;
        out.push(text.all().to_string());
    }
    Ok(out)
}

pub struct ExtractOutcome {
    pub texts: Vec<String>,
    pub ocr_pages: usize,
}

// 渲染整页为 BGRA 位图并送 OCR(Windows); 专用线程规避线程池 COM 单元问题; 失败返回 None
#[cfg(windows)]
fn ocr_page(page: &PdfPage) -> Option<OcrPageResult> {
    let config = PdfRenderConfig::new()
        .set_target_width(1600)
        .set_format(PdfBitmapFormat::BGRA);
    let bitmap = page.render_with_config(&config).ok()?;
    let w = bitmap.width().max(1) as u32;
    let h = bitmap.height().max(1) as u32;
    let bytes = bitmap.as_raw_bytes();
    // WinRT OCR 放专用线程: CoInitializeEx(MTA) 在新线程必然干净成功,
    // 规避 tauri/tokio 线程池 COM 单元状态不确定导致的静默失败
    let handle = std::thread::spawn(move || crate::ocr::recognize_bgra(&bytes, w, h));
    match handle.join() {
        Ok(Ok(r)) => Some(r),
        Ok(Err(e)) => {
            eprintln!("[PDFind][OCR] 识别失败: {e}");
            None
        }
        Err(_) => {
            eprintln!("[PDFind][OCR] OCR 线程崩溃");
            None
        }
    }
}

// 页面是否含大面积图片对象(单图面积 ≥ 版面 30%)
#[cfg(windows)]
fn has_large_image(page: &PdfPage) -> bool {
    let pw = page.width().value;
    let ph = page.height().value;
    let page_area = pw * ph;
    if page_area <= 0.0 {
        return false;
    }
    for obj in page.objects().iter() {
        if obj.object_type() == PdfPageObjectType::Image {
            if let Ok(b) = obj.bounds() {
                if b.width().value * b.height().value / page_area >= 0.3 {
                    return true;
                }
            }
        }
    }
    false
}

// 提取文本(带缓存); use_ocr=true 时对图片页 OCR 并写缓存; cancel 可选逐页检查, OCR 中途取消返回 Err("已取消")
pub fn extract_pages_cached(
    app: &tauri::AppHandle,
    pdfium: &Pdfium,
    path: &Path,
    cancel: Option<&AtomicBool>,
    use_ocr: bool,
) -> Result<ExtractOutcome, String> {
    let key = cache::key_for(path);

    // 1. 缓存整本命中: 免 pdfium 提取, 含 OCR 文本
    if let Some(pages) = cache::get(app, &key, path) {
        let texts = pages.iter().map(|p| p.text.clone()).collect();
        return Ok(ExtractOutcome {
            texts,
            ocr_pages: 0,
        });
    }

    // 2. 提取 + 按需 OCR
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| format!("无法读取 PDF: {e}"))?;
    let mut pages: Vec<OcrPageResult> = Vec::new();
    let mut ocr_pages = 0usize;
    for page in doc.pages().iter() {
        if let Some(f) = cancel {
            if f.load(Ordering::SeqCst) {
                return Err("已取消".into());
            }
        }
        let text = page
            .text()
            .map(|t| t.all().to_string())
            .unwrap_or_default();
        let mut pr = OcrPageResult {
            text,
            words: Vec::new(),
        };
        // 精确触发: 文本稀少 && 存在大面积图片对象(排除每页都有的背景装饰图)
        #[cfg(windows)]
        if use_ocr && pr.text.chars().count() < 300 && has_large_image(&page) {
            if let Some(r) = ocr_page(&page) {
                // 仅当 OCR 文本更长时替换, 防止误触发劣化文本页
                if r.text.chars().count() > pr.text.chars().count() {
                    ocr_pages += 1;
                    pr = r;
                }
            }
        }
        pages.push(pr);
    }

    // 3. OCR 有产出才写缓存(纯文本文件不持久化)
    if ocr_pages > 0 {
        cache::put(app, &key, path, pages.clone());
    }

    let texts = pages.into_iter().map(|p| p.text).collect();
    Ok(ExtractOutcome { texts, ocr_pages })
}
