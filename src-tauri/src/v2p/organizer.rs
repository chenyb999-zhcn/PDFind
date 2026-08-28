// 视频转 PDF: 在线整理大模型的 API Key/配置存储 (后端配置文件)
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OrganizerConfig {
    // 服务商 id -> API Key
    #[serde(default)]
    pub keys: std::collections::HashMap<String, String>,
    // 服务商 id -> 覆盖的 model 名 (如豆包 endpoint id)
    #[serde(default)]
    pub models: std::collections::HashMap<String, String>,
    // 自定义服务商配置
    #[serde(default)]
    pub custom: CustomConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

fn config_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("organizer.json")
}

pub fn load(app: &tauri::AppHandle) -> OrganizerConfig {
    std::fs::read_to_string(config_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &tauri::AppHandle, cfg: &OrganizerConfig) -> Result<(), String> {
    let p = config_path(app);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let s = serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&p, s).map_err(|e| format!("写入配置失败: {e}"))
}
