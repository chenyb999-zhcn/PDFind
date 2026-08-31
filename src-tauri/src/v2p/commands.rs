// 视频转 PDF: tauri 命令 (模型管理 + 设备 + 转写 + PDF)
use crate::state::SearchState;
use crate::v2p::{asr, device, download, ffmpeg, llamacpp, llm, models, organizer, pdf};
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
    pub base_url: String,
    pub default_model: String,
    pub needs_model: bool,
    pub has_key: bool,   // 是否已配置 API Key
    pub models: Vec<String>, // 预置模型下拉列表
}

#[derive(Serialize, Clone)]
pub struct V2pUpdateInfo {
    pub has_update: bool,
    pub new_version: u64,
}

// 检查 v2p 环境: ffmpeg / 模型清单(含下载状态) / 设备 / 整理服务商
#[tauri::command]
pub fn v2p_check_env(app: tauri::AppHandle) -> V2pEnv {
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
    let oc = organizer::load(&app);
    let organizers = models::organizer_providers()
        .iter()
        .map(|p| OrganizerInfo {
            id: p.id.clone(),
            name: p.name.clone(),
            base_url: p.base_url.clone(),
            default_model: p.default_model.clone(),
            needs_model: p.needs_model,
            has_key: oc.keys.contains_key(&p.id),
            models: p.models.clone(),
        })
        .collect();
    V2pEnv {
        ffmpeg: ffmpeg::ffmpeg_path().is_some(),
        models: models_list,
        device: device_info,
        catalog_version: catalog.version,
        has_update: false,
        organizers,
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

// 读取整理服务商配置(含 API Key, 本机应用)
#[tauri::command]
pub fn v2p_get_organizer_config(app: AppHandle) -> organizer::OrganizerConfig {
    organizer::load(&app)
}

// 保存整理服务商配置
#[tauri::command]
pub fn v2p_set_organizer_config(
    app: AppHandle,
    config: organizer::OrganizerConfig,
) -> Result<(), String> {
    organizer::save(&app, &config)
}

// 动态拉取服务商的模型列表 (GET {base_url}/models, OpenAI 兼容)
#[tauri::command]
pub fn v2p_list_organizer_models(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<String>, String> {
    let cfg = organizer::load(&app);
    let (base_url, api_key) = if provider_id == "custom" {
        let c = &cfg.custom;
        if c.base_url.is_empty() || c.api_key.is_empty() {
            return Err("自定义服务商需填 Base URL 和 API Key".into());
        }
        (c.base_url.clone(), c.api_key.clone())
    } else {
        let p = models::organizer_providers()
            .into_iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| format!("未知服务商: {provider_id}"))?;
        let key = cfg.keys.get(&provider_id).cloned().unwrap_or_default();
        if key.is_empty() {
            return Err(format!("{} 的 API Key 未配置", p.name));
        }
        (p.base_url.clone(), key)
    };
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {api_key}"))
        .call()
        .map_err(|e| format!("获取模型列表失败: {e}"))?;
    let status = resp.status();
    if status != 200 {
        let t = resp.into_string().unwrap_or_default();
        return Err(format!("获取模型列表返回 HTTP {status}: {}", &t.chars().take(200).collect::<String>()));
    }
    let json: serde_json::Value = resp.into_json().map_err(|e| format!("解析失败: {e}"))?;
    let list: Vec<String> = json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if list.is_empty() {
        return Err("模型列表为空".into());
    }
    Ok(list)
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

// 在线大模型整理: 调用 OpenAI 兼容 chat/completions, 把转写稿整理为结构化章节
fn organize_with_llm(
    app: &AppHandle,
    transcript: &str,
    lang: &str,
    provider_id: &str,
) -> Result<String, String> {
    let cfg = organizer::load(app);
    let (base_url, model, api_key) = llm::resolve_llm(&cfg, provider_id)?;
    emit_log(app, &format!("调用 {} 在线整理… ({model})", provider_id));
    let lang_hint = if lang == "en" {
        "用英文整理"
    } else if lang == "ja" {
        "用日文整理"
    } else {
        "用中文整理"
    };
    let sys = format!(
        "你是一个文档整理助手。请{lang_hint}，把下面的语音转写稿整理成条理清晰的结构化内容：按主题分成若干章节，每章节给出标题和要点列表。直接输出整理结果，不要多余解释。"
    );
    llm::llm_chat(
        &base_url,
        &model,
        &api_key,
        &[
            ("system".to_string(), sys),
            ("user".to_string(), transcript.to_string()),
        ],
        0.6,
        4096,
        300,
    )
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
    organize: bool,         // 是否用在线大模型整理成章节
    organizer_id: String,   // 选择的整理服务商 id
    save_to_kb: bool,       // 生成后存入知识库
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

    // 在线大模型整理 (大纲另存一份, 供知识库 meta 展示)
    let mut outline_md: Option<String> = None;
    let mut chapters = if organize {
        emit_log(&app, "在线大模型整理转写稿…");
        match organize_with_llm(&app, &transcript, &lang, &organizer_id) {
            Ok(md) => {
                emit_log(&app, "LLM 整理完成");
                outline_md = Some(md.clone());
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

    // 存入知识库 (失败不影响 PDF 生成结果)
    if save_to_kb {
        emit_log(&app, "存入知识库…");
        match crate::kb::db::Db::open(&app) {
            Ok(d) => match crate::kb::ingest::ingest_video(
                &d,
                &media_path,
                Some(&out_pdf),
                &lang,
                &segments,
                outline_md.as_deref(),
                &mut |l| emit_log(&app, l),
            ) {
                Ok(r) => emit_log(&app, &format!(
                    "已存入知识库: {} 块{}",
                    r.chunks,
                    if r.embedded { " (含向量)" } else { " (无向量, embedding 模型未下载)" }
                )),
                Err(e) => emit_log(&app, &format!("存入知识库失败: {e}")),
            },
            Err(e) => emit_log(&app, &format!("知识库打开失败: {e}")),
        }
    }
    Ok(())
}

fn fmt_ts(s: f32) -> String {
    let m = (s as u64) / 60;
    let sec = (s as u64) % 60;
    format!("{:02}:{:02}", m, sec)
}
