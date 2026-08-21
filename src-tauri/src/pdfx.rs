// pdfium 动态库加载与文本提取
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::Manager;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

// 按优先级尝试的 dll 候选路径:打包资源目录 > exe 同目录 > 开发时 src-tauri/binaries
fn dll_candidates(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(rd) = resource_dir {
        v.push(rd.join("pdfium.dll"));
        v.push(rd.join("binaries").join("pdfium.dll"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("pdfium.dll"));
        }
    }
    v.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("pdfium.dll"),
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
