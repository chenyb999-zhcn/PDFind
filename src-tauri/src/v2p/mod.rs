// 视频转 PDF 模块: ASR 转写 + 取帧 + 排版
pub mod asr;
pub mod chapters;
pub mod commands;
pub mod ffmpeg;
#[cfg(windows)]
pub mod ocr;
pub mod pdf;
