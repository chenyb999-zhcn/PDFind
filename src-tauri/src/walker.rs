// 目录递归收集 PDF 文件(支持取消)
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn collect_pdfs(root: &Path, cancel: &AtomicBool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkBuilder::new(root).build().flatten() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let p = entry.path();
        if p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            out.push(p.to_path_buf());
        }
    }
    out
}
