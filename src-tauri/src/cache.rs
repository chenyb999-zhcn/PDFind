// OCR 结果缓存: 键 = 规范化路径, 校验 size + mtime; JSON 持久化到应用缓存目录
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OcrWordBox {
    pub text: String,
    // 归一化坐标 0-1 (相对页面)
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct OcrPageResult {
    pub text: String,
    pub words: Vec<OcrWordBox>,
}

#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    size: u64,
    mtime_ms: u64,
    pages: Vec<OcrPageResult>,
}

struct CacheState {
    data: HashMap<String, CacheEntry>,
    file: PathBuf,
}

static STATE: Mutex<Option<CacheState>> = Mutex::new(None);

pub fn key_for(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn mtime_ms(path: &Path) -> Option<u64> {
    let md = std::fs::metadata(path).ok()?;
    md.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

fn load(app: &tauri::AppHandle) {
    use tauri::Manager;
    let mut g = STATE.lock().unwrap();
    if g.is_some() {
        return;
    }
    let file = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("ocr-cache.json");
    let data = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    *g = Some(CacheState { data, file });
}

// 命中返回整本页结果(文本+OCR词框), size/mtime 不匹配视为失效
pub fn get(app: &tauri::AppHandle, key: &str, path: &Path) -> Option<Vec<OcrPageResult>> {
    load(app);
    let g = STATE.lock().unwrap();
    let e = g.as_ref()?.data.get(key)?;
    if std::fs::metadata(path).ok()?.len() != e.size {
        return None;
    }
    if mtime_ms(path)? != e.mtime_ms {
        return None;
    }
    Some(e.pages.clone())
}

pub fn put(app: &tauri::AppHandle, key: &str, path: &Path, pages: Vec<OcrPageResult>) {
    load(app);
    let (Some(size), Some(mt)) = (std::fs::metadata(path).ok().map(|m| m.len()), mtime_ms(path))
    else {
        return;
    };
    let mut g = STATE.lock().unwrap();
    if let Some(st) = g.as_mut() {
        st.data.insert(
            key.to_string(),
            CacheEntry {
                size,
                mtime_ms: mt,
                pages,
            },
        );
        let _ = std::fs::write(&st.file, serde_json::to_string(&st.data).unwrap_or_default());
    }
}
