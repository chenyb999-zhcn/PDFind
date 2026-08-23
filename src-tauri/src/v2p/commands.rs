// 视频转 PDF: tauri 命令 (转写阶段)
use crate::state::SearchState;
use crate::v2p::{asr, ffmpeg};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, Clone)]
pub struct V2pProgress {
    pub stage: String, // "audio" | "transcribe"
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Serialize, Clone)]
pub struct V2pDone {
    pub segments: usize,
    pub chars: usize,
}

#[derive(Serialize, Clone)]
pub struct V2pEnv {
    pub ffmpeg: bool,
    pub model_dir: String,
    pub model_ready: bool,
    pub engines: Vec<String>,
}

// 检查 v2p 环境 (ffmpeg / 模型)
#[tauri::command]
pub fn v2p_check_env(app: AppHandle) -> V2pEnv {
    let model_dir = model_dir(&app);
    let model_ready = model_dir.join("llm.int8.onnx").exists();
    V2pEnv {
        ffmpeg: ffmpeg::ffmpeg_path().is_some(),
        model_dir: model_dir.display().to_string(),
        model_ready,
        engines: vec!["fun_asr_nano".into(), "paraformer".into()],
    }
}

fn model_dir(_app: &AppHandle) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dev-models")
        .join("sherpa-onnx-funasr-nano-int8-2025-12-30")
}

// 转写命令: 媒体文件 -> wav -> ASR 切片转写, 事件流进度, 支持取消
#[tauri::command]
pub async fn v2p_transcribe(
    app: AppHandle,
    state: State<'_, SearchState>,
    media_path: String,
    engine: String,
) -> Result<V2pDone, String> {
    let cancel = state.begin().ok_or("已有任务在进行中")?;
    let result = run_transcribe(&app, &media_path, &engine, cancel.clone());
    state.end();
    result
}

fn run_transcribe(
    app: &AppHandle,
    media_path: &str,
    engine: &str,
    cancel: Arc<AtomicBool>,
) -> Result<V2pDone, String> {
    let media = PathBuf::from(media_path);
    if !media.exists() {
        return Err("媒体文件不存在".into());
    }

    // 1. 提取音频到临时 wav
    let tmp_wav = std::env::temp_dir().join(format!(
        "pdfind_v2p_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    let _ = app.emit(
        "v2p:progress",
        V2pProgress {
            stage: "audio".into(),
            done: 0,
            total: 1,
            current: "提取音频…".into(),
        },
    );
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".into());
    }
    ffmpeg::extract_audio(&media, &tmp_wav)?;
    let _ = app.emit(
        "v2p:progress",
        V2pProgress {
            stage: "audio".into(),
            done: 1,
            total: 1,
            current: "音频提取完成".into(),
        },
    );

    // 2. ASR 切片转写 (带进度 + 取消)
    let model_dir = model_dir(app);
    let asr_engine = asr::AsrEngine::from_str(engine);
    let asr = asr::Asr::create(asr_engine, model_dir.to_str().unwrap_or_default())?;

    let wave = sherpa_onnx::Wave::read(tmp_wav.to_str().unwrap_or_default())
        .ok_or_else(|| "读取音频失败".to_string())?;

    let mut cancelled = false;
    let segments = asr
        .transcribe_wave_with_progress(&wave, &mut |done, total_chunks| {
            if cancel.load(Ordering::SeqCst) {
                cancelled = true;
                return false;
            }
            let _ = app.emit(
                "v2p:progress",
                V2pProgress {
                    stage: "transcribe".into(),
                    done,
                    total: total_chunks,
                    current: format!("转写中 ({done}/{total_chunks})"),
                },
            );
            true
        })
        .map_err(|e| e)?;

    let _ = std::fs::remove_file(&tmp_wav);
    if cancelled {
        return Err("已取消".into());
    }
    let chars: usize = segments.iter().map(|s| s.text.chars().count()).sum();
    Ok(V2pDone {
        segments: segments.len(),
        chars,
    })
}
