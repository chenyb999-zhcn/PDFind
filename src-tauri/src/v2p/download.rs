// 视频转 PDF: 模型下载器 (ModelScope 优先, HF 回退) + tar.bz2 解压 + 远端清单
use super::models::{AsrModel, ModelFile, ModelsCatalog, REMOTE_CATALOG_URL};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct DlProgress {
    pub model_id: String,
    pub file: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub current_file_idx: usize,
    pub total_files: usize,
}

fn models_dir() -> PathBuf {
    super::models::model_dir()
}

fn download_file(url: &str, dest: &PathBuf) -> Result<(), String> {
    let mut resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| format!("下载失败 {url}: {e}"))?;
    let mut f = File::create(dest).map_err(|e| format!("创建文件失败: {e}"))?;
    std::io::copy(&mut resp.into_reader(), &mut f).map_err(|e| format!("写入失败: {e}"))?;
    Ok(())
}

// 提取下载地址: ModelScope 优先, 空则 HF; 两者都空返回 None
fn pick_url(f: &ModelFile) -> Option<String> {
    if !f.url_ms.is_empty() {
        Some(f.url_ms.clone())
    } else if !f.url_hf.is_empty() {
        Some(f.url_hf.clone())
    } else {
        None
    }
}

// 解压 tar.bz2 到目标目录
fn extract_tar_bz2(archive: &PathBuf, dest_dir: &PathBuf) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let bz = bzip2::read::BzDecoder::new(file);
    let mut ar = tar::Archive::new(bz);
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("创建目录失败: {e}"))?;
    ar.unpack(dest_dir)
        .map_err(|e| format!("解压失败: {e}"))?;
    Ok(())
}

// 下载模型的所有文件到 <models>/<model_id>/; 若 extract_tar 则解压后删除压缩包
pub fn download_model(
    app: &AppHandle,
    model: &AsrModel,
    on_progress: &mut dyn FnMut(DlProgress),
) -> Result<(), String> {
    let dir = models_dir().join(&model.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建模型目录失败: {e}"))?;

    let total = model.files.len();
    for (idx, f) in model.files.iter().enumerate() {
        let dest = dir.join(&f.name);
        if dest.exists() {
            continue; // 已存在跳过
        }
        let url = pick_url(f).ok_or_else(|| format!("模型 {} 缺下载地址", f.name))?;
        on_progress(DlProgress {
            model_id: model.id.clone(),
            file: f.name.clone(),
            done_bytes: 0,
            total_bytes: f.size_mb * 1024 * 1024,
            current_file_idx: idx + 1,
            total_files: total,
        });
        download_file(&url, &dest)?;
        on_progress(DlProgress {
            model_id: model.id.clone(),
            file: f.name.clone(),
            done_bytes: f.size_mb * 1024 * 1024,
            total_bytes: f.size_mb * 1024 * 1024,
            current_file_idx: idx + 1,
            total_files: total,
        });
    }

    // 需要解压: 若第一个文件是 tar.bz2, 解压到 model_id 目录
    if model.extract_tar {
        if let Some(first) = model.files.first() {
            let archive = dir.join(&first.name);
            if archive.exists() && first.name.ends_with(".tar.bz2") {
                extract_tar_bz2(&archive, &dir)?;
                // 解压后删除压缩包
                let _ = std::fs::remove_file(&archive);
            }
        }
    }
    Ok(())
}

// 拉取远端清单; 返回 Some(新清单) 若版本更新
pub fn fetch_remote_catalog() -> Option<ModelsCatalog> {
    let resp = ureq::get(REMOTE_CATALOG_URL)
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    serde_json::from_str(&text).ok()
}

// 检查版本更新: 返回远端清单(若比本地 version 新)
pub fn check_catalog_update(local: &ModelsCatalog) -> Option<ModelsCatalog> {
    let remote = fetch_remote_catalog()?;
    if remote.version > local.version {
        Some(remote)
    } else {
        None
    }
}
