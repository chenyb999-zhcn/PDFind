// 知识库: 本地 embedding (调 llama-funasr-cli --embed 批量编码)
use crate::v2p::llamacpp::LlamaCli;
use serde_json::Value;
use std::path::PathBuf;

// embedding 模型元数据 (随版本发布的固定模型, 不走 ASR 模型清单)
pub const EMBED_MODEL_DIR: &str = "bge-small-zh-v1.5";
pub const EMBED_MODEL_FILE: &str = "bge-small-zh-v1.5-q8_0.gguf";
pub const EMBED_MODEL_SIZE_MB: u64 = 26;
pub const EMBED_MODEL_URL: &str =
    "https://github.com/chenyb999-zhcn/PDFind/releases/download/v0.4.0/bge-small-zh-v1.5-q8_0.gguf";

// 模型路径与 ASR 模型同目录体系 (dev-models/<id>/<file>)
pub fn embed_model_path() -> PathBuf {
    crate::v2p::models::model_dir().join(EMBED_MODEL_DIR).join(EMBED_MODEL_FILE)
}

pub fn embed_model_downloaded() -> bool {
    embed_model_path().exists()
}

// 下载 embedding 模型 (on_progress: done_bytes, total_bytes)
pub fn download_embed_model(on_progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let dest = embed_model_path();
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let resp = ureq::get(EMBED_MODEL_URL)
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| format!("下载失败: {e}"))?;
    let total = resp
        .header("content-length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(EMBED_MODEL_SIZE_MB * 1024 * 1024);
    let tmp = dest.with_extension("gguf.part");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut reader = resp.into_reader();
        let mut done: u64 = 0;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            use std::io::Read;
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    std::io::Write::write_all(&mut f, &buf[..n]).map_err(|e| e.to_string())?;
                    done += n as u64;
                    on_progress(done, total);
                }
                Err(e) => return Err(format!("读取失败: {e}")),
            }
        }
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

// 批量编码: texts[i] -> 归一化向量; 顺序与输入一致
pub fn embed_texts(texts: &[String], on_log: &mut dyn FnMut(&str)) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let model = embed_model_path();
    if !model.exists() {
        return Err("embedding 模型未下载".into());
    }
    let cli = LlamaCli::find("llama-funasr-cli", false)
        .ok_or("未找到 llama-funasr-cli 可执行文件")?;

    // 写输入文件: 每行一条文本 (换行符压成空格)
    let tmp_in = std::env::temp_dir().join(format!("pdfind_kb_embed_{}.txt", std::process::id()));
    let mut content = String::new();
    for t in texts {
        let one: String = t.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
        content.push_str(one.trim());
        content.push('\n');
    }
    std::fs::write(&tmp_in, &content).map_err(|e| e.to_string())?;

    let mut cmd = std::process::Command::new(cli.bin_path());
    cmd.args(["-m", model.to_str().unwrap_or_default(), "--embed", "--input", tmp_in.to_str().unwrap_or_default()]);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    let child = cmd.spawn().map_err(|e| format!("启动 embedding CLI 失败: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp_in);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("embedding CLI 失败: {}", err.chars().take(300).collect::<String>()));
    }
    on_log(&format!("embedding {} 条文本完成", texts.len()));

    // 解析 stdout JSONL, 按 i 对齐
    let mut result: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let i = v.get("i").and_then(|x| x.as_u64()).unwrap_or(u64::MAX) as usize;
        let vec: Vec<f32> = v
            .get("v")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect())
            .unwrap_or_default();
        if i < result.len() && !vec.is_empty() {
            result[i] = Some(vec);
        }
    }
    result
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            v.ok_or_else(|| format!("第 {i} 条文本编码失败"))
        })
        .collect()
}
