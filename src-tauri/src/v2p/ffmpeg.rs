// 视频转 PDF: ffmpeg 封装 (音频提取 + 取帧)
use std::path::{Path, PathBuf};
use std::process::Command;

// ffmpeg 可执行文件候选路径: 打包资源目录 > exe 同目录 > 开发时 msys2 mingw64
pub fn ffmpeg_path() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("ffmpeg.exe"));
            cands.push(dir.join("binaries").join("ffmpeg.exe"));
        }
    }
    cands.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("dev-ffmpeg")
            .join("ffmpeg.exe"),
    );
    cands.push(PathBuf::from(r"C:\msys64\mingw64\bin\ffmpeg.exe"));
    for c in cands {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

// 提取音频为 16kHz 单声道 wav (sherpa-onnx 输入要求)
pub fn extract_audio(video: &Path, out_wav: &Path) -> Result<(), String> {
    let ff = ffmpeg_path().ok_or_else(|| "未找到 ffmpeg.exe".to_string())?;
    let status = Command::new(&ff)
        .args([
            "-y",
            "-i",
            video.to_str().unwrap_or_default(),
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            out_wav.to_str().unwrap_or_default(),
        ])
        .status()
        .map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("ffmpeg 音频提取失败".into())
    }
}

// 场景检测取帧: 返回帧文件列表
pub fn extract_scene_frames(video: &Path, out_dir: &Path, threshold: f32) -> Result<Vec<PathBuf>, String> {
    let ff = ffmpeg_path().ok_or_else(|| "未找到 ffmpeg.exe".to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("创建帧目录失败: {e}"))?;
    let pattern = out_dir.join("scene_%04d.jpg");
    let status = Command::new(&ff)
        .args([
            "-y",
            "-i",
            video.to_str().unwrap_or_default(),
            "-vf",
            &format!("select='gt(scene,{threshold})',scale=960:-1"),
            "-fps_mode",
            "vfr",
            "-q:v",
            "3",
            pattern.to_str().unwrap_or_default(),
        ])
        .status()
        .map_err(|e| format!("ffmpeg 取帧失败: {e}"))?;
    if !status.success() {
        return Err("ffmpeg 场景取帧失败".into());
    }
    // 收集帧文件
    let mut frames = Vec::new();
    let mut n = 1;
    loop {
        let p = out_dir.join(format!("scene_{n:04}.jpg"));
        if p.is_file() {
            frames.push(p);
            n += 1;
        } else {
            break;
        }
    }
    Ok(frames)
}

// 指定时间点取单帧
pub fn extract_frame_at(video: &Path, out_dir: &Path, seconds: f32) -> Result<PathBuf, String> {
    let ff = ffmpeg_path().ok_or_else(|| "未找到 ffmpeg.exe".to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("创建帧目录失败: {e}"))?;
    let out = out_dir.join(format!("key_{:.0}.jpg", seconds));
    let status = Command::new(&ff)
        .args([
            "-y",
            "-ss",
            &format!("{seconds}"),
            "-i",
            video.to_str().unwrap_or_default(),
            "-frames:v",
            "1",
            "-q:v",
            "3",
            out.to_str().unwrap_or_default(),
        ])
        .status()
        .map_err(|e| format!("ffmpeg 取帧失败: {e}"))?;
    if status.success() && out.is_file() {
        Ok(out)
    } else {
        Err("ffmpeg 关键帧提取失败".into())
    }
}
