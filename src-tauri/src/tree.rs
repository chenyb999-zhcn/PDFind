// 目录树数据: 列出子目录与 PDF 文件(资源管理器风格排序), 懒加载供前端展开
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub has_children: bool,
}

// 跳过隐藏项(点开头)与各平台系统垃圾目录
fn is_junk(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
        || name.eq_ignore_ascii_case("RECYCLER")
        || name.eq_ignore_ascii_case("lost+found")
}

fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

// 探测目录下是否存在有效子项(子目录或 PDF), 命中一个即返回
fn probe_has_children(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else {
        return false;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if is_junk(&entry.file_name().to_string_lossy()) {
            continue;
        }
        if ft.is_dir() || (ft.is_file() && is_pdf(&entry.path())) {
            return true;
        }
    }
    false
}

fn list_dir_sync(path: &str) -> Result<Vec<TreeEntry>, String> {
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err("路径不是目录".into());
    }
    let rd = fs::read_dir(&root).map_err(|e| format!("无法读取目录: {e}"))?;
    let mut dirs: Vec<TreeEntry> = Vec::new();
    let mut files: Vec<TreeEntry> = Vec::new();
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_junk(&name) {
            continue;
        }
        let p = entry.path();
        if ft.is_dir() {
            dirs.push(TreeEntry {
                name,
                path: p.display().to_string(),
                is_dir: true,
                has_children: probe_has_children(&p),
            });
        } else if ft.is_file() && is_pdf(&p) {
            files.push(TreeEntry {
                name,
                path: p.display().to_string(),
                is_dir: false,
                has_children: false,
            });
        }
    }
    // 目录在前文件在后, 名称不区分大小写
    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(dirs.into_iter().chain(files).collect())
}

#[tauri::command]
pub async fn list_tree_dir(path: String) -> Result<Vec<TreeEntry>, String> {
    // 目录遍历放阻塞线程池, 避免慢速盘(网络盘)卡住界面
    tauri::async_runtime::spawn_blocking(move || list_dir_sync(&path))
        .await
        .map_err(|e| format!("任务失败: {e}"))?
}
