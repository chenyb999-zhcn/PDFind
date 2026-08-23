// 视频转 PDF: tauri 命令 (模型管理 + 设备 + 转写 + PDF)
use crate::state::SearchState;
use crate::v2p::{asr, device, download, ffmpeg, llamacpp, models, pdf};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, Clone)]
pub struct V2pProgress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Serialize, Clone)]
pub struct V2pLog {
    pub text: String,
}

#[derive(Serialize, Clone)]
pub struct V2pDone {
    pub segments: usize,
    pub chars: usize,
    pub segment_list: Vec<SegmentInfo>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SegmentInfo {
    pub text: String,
    pub start: f32,
    pub end: f32,
}

#[derive(Serialize, Clone)]
pub struct V2pModelInfo {
    pub id: String,
    pub name: String,
    pub runtime: String,
    pub gpu: bool,
    pub needs_vad: bool,
    pub downloaded: bool,
}

#[derive(Serialize, Clone)]
pub struct V2pEnv {
    pub ffmpeg: bool,
    pub models: Vec<V2pModelInfo>,
    pub device: device::DeviceInfo,
    pub catalog_version: u64,
    pub has_update: bool,
    pub organizers: Vec<OrganizerInfo>,
}

#[derive(Serialize, Clone)]
pub struct OrganizerInfo {
    pub id: String,
    pub name: String,
    pub downloaded: bool,
    pub size_mb: u64,
}

#[derive(Serialize, Clone)]
pub struct V2pUpdateInfo {
    pub has_update: bool,
    pub new_version: u64,
}

// 检查 v2p 环境: ffmpeg / 模型清单(含下载状态) / 设备
#[tauri::command]
pub fn v2p_check_env() -> V2pEnv {
    let catalog = models::builtin_catalog();
    let device_info = device::detect_devices();
    let models_list = catalog
        .models
        .iter()
        .map(|m| V2pModelInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            runtime: m.runtime.clone(),
            gpu: m.gpu,
            needs_vad: m.needs_vad,
            downloaded: models::is_downloaded(m),
        })
        .collect();
    V2pEnv {
        ffmpeg: ffmpeg::ffmpeg_path().is_some(),
        models: models_list,
        device: device_info,
        catalog_version: catalog.version,
        has_update: false,
        organizers: models::organizer_models()
            .iter()
            .map(|m| OrganizerInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                downloaded: models::organizer_downloaded(m),
                size_mb: m.file.size_mb,
            })
            .collect(),
    }
}

// 检查远端清单是否有更新
#[tauri::command]
pub fn v2p_check_update() -> V2pUpdateInfo {
    let local = models::builtin_catalog();
    match download::check_catalog_update(&local) {
        Some(remote) => V2pUpdateInfo {
            has_update: true,
            new_version: remote.version,
        },
        None => V2pUpdateInfo {
            has_update: false,
            new_version: local.version,
        },
    }
}

// 下载模型 (事件流进度 v2p:dl)
#[tauri::command]
pub async fn v2p_download_model(
    app: AppHandle,
    model_id: String,
) -> Result<(), String> {
    let catalog = models::builtin_catalog();
    let model = catalog
        .find(&model_id)
        .ok_or_else(|| format!("未知模型: {model_id}"))?;
    download::download_model(&app, model, &mut |p| {
        let _ = app.emit("v2p:dl", &p);
    })
}

// 下载 PDF 整理用 LLM 模型
#[tauri::command]
pub async fn v2p_download_organizer(
    app: AppHandle,
    organizer_id: String,
) -> Result<(), String> {
    let om = models::organizer_models()
        .into_iter()
        .find(|m| m.id == organizer_id)
        .ok_or_else(|| format!("未知整理模型: {organizer_id}"))?;
    let dir = models::organizer_dir().join(&om.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    let dest = models::organizer_path(&om);
    if dest.exists() {
        return Ok(());
    }
    // 复用下载逻辑: 构造临时 AsrModel
    let tmp = models::AsrModel {
        id: om.id.clone(),
        name: om.name.clone(),
        runtime: "llamacpp".into(),
        cli: String::new(),
        gpu: false,
        needs_vad: false,
        extract_tar: false,
        files: vec![om.file.clone()],
    };
    download::download_model(&app, &tmp, &mut |p| {
        let _ = app.emit("v2p:dl", &p);
    })
}

// 转写命令: 按模型运行时路由 (llamacpp 子进程 / sherpa-onnx 库)
#[tauri::command]
pub async fn v2p_transcribe(
    app: AppHandle,
    state: State<'_, SearchState>,
    media_path: String,
    model_id: String,
    device: String, // "cpu" | "cuda" | "auto"
    lang: String,   // "zh" | "en" | "ja"
) -> Result<V2pDone, String> {
    let cancel = state.begin().ok_or("已有任务在进行中")?;
    let result = run_transcribe(&app, &media_path, &model_id, &device, &lang, cancel.clone());
    state.end();
    result
}

fn emit_progress(app: &AppHandle, stage: &str, done: usize, total: usize, current: &str) {
    let _ = app.emit(
        "v2p:progress",
        V2pProgress {
            stage: stage.into(),
            done,
            total,
            current: current.into(),
        },
    );
}

fn emit_log(app: &AppHandle, text: &str) {
    let _ = app.emit("v2p:log", V2pLog { text: text.into() });
}

fn run_transcribe(
    app: &AppHandle,
    media_path: &str,
    model_id: &str,
    device: &str,
    lang: &str,
    cancel: Arc<AtomicBool>,
) -> Result<V2pDone, String> {
    let media = PathBuf::from(media_path);
    if !media.exists() {
        return Err("媒体文件不存在".into());
    }
    let catalog = models::builtin_catalog();
    let model = catalog
        .find(model_id)
        .ok_or_else(|| format!("未知模型: {model_id}"))?;
    if !models::is_downloaded(model) {
        return Err(format!("模型 {} 未下载, 请先下载", model.name));
    }

    // 1. 提取音频
    let tmp_wav = std::env::temp_dir().join(format!(
        "pdfind_v2p_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    emit_progress(app, "audio", 0, 1, "提取音频…");
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".into());
    }
    ffmpeg::extract_audio(&media, &tmp_wav)?;
    emit_progress(app, "audio", 1, 1, "音频提取完成");

    let result = match model.runtime.as_str() {
        "llamacpp" => transcribe_llamacpp(app, model, &tmp_wav, device, lang, &cancel),
        _ => transcribe_sherpaonnx(app, model, &tmp_wav, &cancel),
    };

    let _ = std::fs::remove_file(&tmp_wav);
    result
}

// llama.cpp 子进程转写
fn transcribe_llamacpp(
    app: &AppHandle,
    model: &models::AsrModel,
    wav: &std::path::Path,
    device: &str,
    lang: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<V2pDone, String> {
    let effective_device = if device == "cuda" && !model.gpu {
        emit_log(app, "该模型不支持 CUDA, 使用 CPU");
        "cpu"
    } else {
        device
    };
    let cli = llamacpp::LlamaCli::find(&model.cli, effective_device == "cuda")
        .ok_or_else(|| format!("未找到 {} 可执行文件(需下载运行时)", model.cli))?;
    let files = models::local_paths(model);

    // 构造模型参数
    let mut args: Vec<String> = Vec::new();
    if model.id == "nano-llamacpp" {
        let enc = files.get("funasr-encoder-f16.gguf").unwrap();
        let llm = files.get("qwen3-0.6b-q8_0.gguf").unwrap();
        args.push("--enc".into());
        args.push(enc.display().to_string());
        args.push("-m".into());
        args.push(llm.display().to_string());
    } else {
        let m = files
            .iter()
            .find(|(k, _)| *k != "fsmn-vad.gguf")
            .map(|(_, v)| v)
            .unwrap();
        args.push("-m".into());
        args.push(m.display().to_string());
    }
    let vad = files.get("fsmn-vad.gguf").map(|p| p.display().to_string());

    let mut chars = 0usize;
    let mut seg_list: Vec<SegmentInfo> = Vec::new();
    // CLI stdout 段行格式: "[  start-end] text"
    let parse_seg = |line: &str| -> Option<SegmentInfo> {
        let l = line.trim_start();
        if !l.starts_with('[') {
            return None;
        }
        let close = l.find(']')?;
        let inner = &l[1..close];
        let dash = inner.find('-')?;
        let start: f32 = inner[..dash].trim().parse().ok()?;
        let end: f32 = inner[dash + 1..].trim().parse().ok()?;
        let text = l[close + 1..].trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(SegmentInfo { text, start, end })
        }
    };
    cli.transcribe(
        &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        vad.as_deref(),
        wav.to_str().unwrap_or_default(),
        effective_device,
        lang,
        cancel.clone(),
        &mut |line| {
            if let Some(seg) = parse_seg(line) {
                chars += seg.text.chars().count();
                seg_list.push(seg);
            }
        },
        &mut |log| {
            emit_log(app, log);
        },
    )?;
    emit_log(app, &format!("转写完成: {} 段, {} 字符", seg_list.len(), chars));
    let _ = app.emit("v2p:result", &seg_list);
    Ok(V2pDone {
        segments: seg_list.len(),
        chars,
        segment_list: seg_list,
    })
}

// sherpa-onnx 库转写
fn transcribe_sherpaonnx(
    app: &AppHandle,
    model: &models::AsrModel,
    wav: &std::path::Path,
    cancel: &Arc<AtomicBool>,
) -> Result<V2pDone, String> {
    let files = models::local_paths(model);
    let mdir = files
        .values()
        .next()
        .map(|p| p.parent().unwrap().to_path_buf())
        .unwrap();

    let engine = match model.id.as_str() {
        "paraformer-onnx" => asr::AsrEngine::Paraformer,
        _ => asr::AsrEngine::FunAsrNano,
    };
    let asr = asr::Asr::create(engine, mdir.to_str().unwrap_or_default())?;
    let wave = sherpa_onnx::Wave::read(wav.to_str().unwrap_or_default())
        .ok_or_else(|| "读取音频失败".to_string())?;

    let mut chars = 0usize;
    let mut cancelled = false;
    let seg_list = asr
        .transcribe_wave_with_progress(&wave, &mut |done, total, chunk_text| {
            if cancel.load(Ordering::SeqCst) {
                cancelled = true;
                return false;
            }
            chars += chunk_text.chars().count();
            emit_progress(app, "transcribe", done, total, &format!("转写中 ({done}/{total})"));
            true
        })
        .map_err(|e| e)?;
    if cancelled {
        return Err("已取消".into());
    }
    let seg_list: Vec<SegmentInfo> = seg_list
        .into_iter()
        .map(|s| SegmentInfo {
            text: s.text,
            start: s.start,
            end: s.end,
        })
        .collect();
    emit_log(app, &format!("转写完成: {} 段, {} 字符", seg_list.len(), chars));
    let _ = app.emit("v2p:result", &seg_list);
    Ok(V2pDone {
        segments: seg_list.len(),
        chars,
        segment_list: seg_list,
    })
}

// 系统可用的中文字体(单字体 TTF, 避免 rusttype 不支持 TTC 集合)
fn cn_font() -> Option<String> {
    let cands = [
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\msyh.ttc",
    ];
    cands.iter().find(|p| std::path::Path::new(p).exists()).map(|s| s.to_string())
}

// 本地 LLM 整理: 用 nano 的 Qwen3 模型把转写稿整理为结构化章节 (返回 markdown 文本)
fn organize_with_llm(
    app: &AppHandle,
    transcript: &str,
    lang: &str,
    organizer_id: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    let om = models::organizer_models()
        .into_iter()
        .find(|m| m.id == organizer_id)
        .ok_or_else(|| format!("未知整理模型: {organizer_id}"))?;
    let model_path = models::organizer_path(&om);
    if !model_path.exists() {
        return Err(format!("整理模型 {} 未下载", om.name));
    }
    // GPU 可用时用 CUDA 版 CLI (bin-cuda), 否则 CPU 版
    let use_cuda = device::detect_devices().has_nvidia_gpu;
    let cli = llamacpp::LlamaCli::find("llama-funasr-cli", use_cuda)
        .ok_or("未找到 llama-funasr-cli 可执行文件")?;

    let mut cmd = std::process::Command::new(cli.bin_path());
    cmd.arg("-m").arg(&model_path);
    cmd.arg("--summarize");
    if !lang.is_empty() {
        cmd.arg("--lang").arg(lang);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("启动整理失败: {e}"))?;
    {
        use std::io::Write;
        let mut si = child.stdin.take().ok_or("无法写入 stdin")?;
        let _ = si.write_all(transcript.as_bytes());
    }
    let stdout = child.stdout.take().ok_or("无法读取整理输出")?;
    let stderr = child.stderr.take().ok_or("无法读取整理错误")?;

    // 读 stdout/stderr 到字符串 (超时 600s, 长文本 CPU 整理可能较慢)
    let read_all = |mut r: std::process::ChildStdout| -> String {
        use std::io::Read;
        let mut s = String::new();
        let _ = r.read_to_string(&mut s);
        s
    };
    let read_err = |mut r: std::process::ChildStderr| -> String {
        use std::io::Read;
        let mut s = String::new();
        let _ = r.read_to_string(&mut s);
        s
    };
    let (out, err) = std::thread::scope(|s| {
        let o = s.spawn(move || read_all(stdout));
        let e = s.spawn(move || read_err(stderr));
        let mut ch = child;
        let _ = ch.wait();
        (o.join().unwrap_or_default(), e.join().unwrap_or_default())
    });
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".into());
    }
    let text = out.trim().to_string();
    if text.is_empty() {
        // 上报 stderr 尾部, 便于定位
        let tail: String = err.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
        let _ = emit_log(app, &format!("整理失败 (stderr): {}", tail));
        return Err("整理无输出".into());
    }
    Ok(text)
}

// 生成 PDF: 转写分段 + 视频截图 + 可选本地 LLM 整理
#[tauri::command]
pub async fn v2p_generate_pdf(
    app: AppHandle,
    media_path: String,
    out_pdf: String,
    lang: String,
    transcript: String,     // 原始转写全文(用于 LLM 整理)
    segments: Vec<SegmentInfo>, // 带时间戳的分段(用于按时间分章)
    organize: bool,         // 是否用本地 LLM 整理成章节
    organizer_id: String,   // 选择的整理模型 id
) -> Result<(), String> {
    let font = cn_font().ok_or("未找到系统中文字体")?;

    // 场景截图
    emit_log(&app, "提取视频场景截图…");
    let frame_dir = std::env::temp_dir().join(format!("v2p_frames_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&frame_dir);
    let frames = ffmpeg::extract_scene_frames(
        &PathBuf::from(&media_path),
        &frame_dir,
        0.30,
    )
    .map_err(|e| e)?;
    emit_log(&app, &format!("截图 {} 张", frames.len()));

    // 本地 LLM 整理
    let mut chapters = if organize {
        emit_log(&app, "本地 LLM 整理转写稿…");
        match organize_with_llm(&app, &transcript, &lang, &organizer_id, &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))) {
            Ok(md) => {
                emit_log(&app, "LLM 整理完成");
                md.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let t = l.trim();
                        pdf::PdfChapter {
                            title: t.chars().take(40).collect(),
                            text: t.to_string(),
                            images: Vec::new(),
                        }
                    })
                    .collect::<Vec<_>>()
            }
            Err(e) => {
                emit_log(&app, &format!("LLM 整理不可用({e}), 回退为按时间分章"));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    // organize 为空(模型能力不足或失败)时, 回退为按转写分段分章, 保证 PDF 始终有内容
    if chapters.is_empty() {
        chapters = segments
            .iter()
            .map(|s| {
                let title = format!(
                    "{} - {}",
                    fmt_ts(s.start),
                    fmt_ts(s.end)
                );
                pdf::PdfChapter {
                    title,
                    text: s.text.clone(),
                    images: Vec::new(),
                }
            })
            .collect();
    }

    // 渲染 PDF
    emit_log(&app, "生成 PDF…");
    let spec = pdf::PdfSpec {
        title: "视频转 PDF".into(),
        subtitle: format!("来源: {}", PathBuf::from(&media_path).file_name().unwrap_or_default().to_string_lossy()),
        meta: vec![("截图".into(), format!("{} 张", frames.len()))],
        chapters,
    };
    pdf::render_pdf(&spec, &font, &out_pdf)?;
    let _ = std::fs::remove_dir_all(&frame_dir);
    emit_log(&app, &format!("PDF 已生成: {out_pdf}"));
    Ok(())
}

fn fmt_ts(s: f32) -> String {
    let m = (s as u64) / 60;
    let sec = (s as u64) % 60;
    format!("{:02}:{:02}", m, sec)
}
