// 视频转 PDF 模块: ASR 转写 + 取帧 + 排版
pub mod asr;
pub mod chapters;
pub mod commands;
pub mod device;
pub mod download;
pub mod ffmpeg;
pub mod llamacpp;
pub mod llm;
pub mod models;
#[cfg(windows)]
pub mod ocr;
pub mod organizer;
pub mod pdf;
