// Windows 系统 OCR (WinRT Windows.Media.Ocr): 图片页文字识别, 含词框坐标
#![cfg(windows)]

use crate::cache::{OcrPageResult, OcrWordBox};
use std::sync::OnceLock;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;

static ENGINE: OnceLock<Option<OcrEngine>> = OnceLock::new();

#[allow(dead_code)]
fn win_err(e: windows::core::Error) -> String {
    format!("WinRT: {e}")
}

fn need_com_mta() -> Result<(), String> {
    // WinRT 异步操作要求 COM 已初始化; 每线程一次
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::core::HRESULT;
    thread_local! {
        static INIT: Result<(), ()> = unsafe {
            let hr: HRESULT = CoInitializeEx(None, COINIT_MULTITHREADED);
            // S_OK(0) / S_FALSE(1,已初始化) 成功; RPC_E_CHANGED_MODE(-2147417850) 视为可继续
            if hr.is_ok() || hr.0 == -2147417850 {
                Ok(())
            } else {
                Err(())
            }
        };
    }
    INIT.with(|r| r.clone().map_err(|()| "COM 初始化失败".to_string()))
}

fn get_engine() -> Result<&'static OcrEngine, String> {
    let slot = ENGINE.get_or_init(|| {
        let lang = Language::CreateLanguage(&HSTRING::from("zh-Hans-CN")).ok();
        let from_lang = lang.as_ref().and_then(|l| {
            if OcrEngine::IsLanguageSupported(l).unwrap_or(false) {
                OcrEngine::TryCreateFromLanguage(l).ok()
            } else {
                None
            }
        });
        let engine = from_lang.or_else(|| OcrEngine::TryCreateFromUserProfileLanguages().ok());
        if engine.is_none() {
            eprintln!("[PDFind][OCR] 引擎创建失败: 无可用语言包(需系统安装中文OCR)");
        }
        engine
    });
    slot.as_ref().ok_or_else(|| "系统无可用 OCR 语言包".to_string())
}

const MAX_DIM: u32 = 4000; // 保守上限, 低于 WinRT OCR 的 MaxImageDimension

/// 输入 pdfium 渲染的 BGRx 像素, 返回识别文本与词框(归一化到原始尺寸)
pub fn recognize_bgra(pixels: &[u8], width: u32, height: u32) -> Result<OcrPageResult, String> {
    need_com_mta()?;
    let engine = get_engine()?;

    // 尺寸钳制: 超限等比缩放(最近邻)
    let (w, h, scale) = if width > MAX_DIM || height > MAX_DIM {
        let s = MAX_DIM as f64 / width.max(height) as f64;
        ((width as f64 * s) as u32, (height as f64 * s) as u32, s)
    } else {
        (width, height, 1.0)
    };

    // 构造 BGRA 缓冲: BGRx -> BGRA (alpha 强制 0xFF), 需要时缩放
    let mut buf: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    if scale == 1.0 {
        buf.extend_from_slice(pixels);
        for c in buf.chunks_exact_mut(4) {
            c[3] = 0xFF;
        }
    } else {
        buf.resize((w * h * 4) as usize, 0xFF);
        let stride_src = (width * 4) as usize;
        let stride_dst = (w * 4) as usize;
        for dy in 0..h as usize {
            let sy = (dy as f64 / scale) as usize;
            let row_src = sy * stride_src;
            let row_dst = dy * stride_dst;
            for dx in 0..w as usize {
                let sx = (dx as f64 / scale) as usize;
                let s = row_src + sx * 4;
                let d = row_dst + dx * 4;
                buf[d] = pixels[s];
                buf[d + 1] = pixels[s + 1];
                buf[d + 2] = pixels[s + 2];
                buf[d + 3] = 0xFF;
            }
        }
    }

    let bitmap = unsafe { bitmap_from_bgra(&buf, w, h)? };

    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| format!("OCR 启动失败: {e}"))?
        .get()
        .map_err(|e| format!("OCR 识别失败: {e}"))?;

    let mut out = OcrPageResult::default();
    let fw = width as f64;
    let fh = height as f64;
    let inv = 1.0 / scale; // 词框坐标反缩放回原始尺寸再归一化
    for line in result.Lines().map_err(win_err)? {
        let line_text = line.Text().map_err(win_err)?.to_string();
        let mut line_words = Vec::new();
        for word in line.Words().map_err(win_err)? {
            let r = word.BoundingRect().map_err(win_err)?;
            let t = word.Text().map_err(win_err)?.to_string();
            line_words.push(OcrWordBox {
                text: t,
                x: ((r.X as f64 * inv) / fw).max(0.0),
                y: ((r.Y as f64 * inv) / fh).max(0.0),
                w: ((r.Width as f64 * inv) / fw).min(1.0),
                h: ((r.Height as f64 * inv) / fh).min(1.0),
            });
        }
        if !line_text.trim().is_empty() {
            if !out.text.is_empty() {
                out.text.push('\n');
            }
            out.text.push_str(line_text.trim());
        }
        out.words.append(&mut line_words);
    }
    Ok(out)
}

/// 像素缓冲 -> SoftwareBitmap: WinRT IBuffer 共享内存路径
unsafe fn bitmap_from_bgra(buf: &[u8], w: u32, h: u32) -> Result<SoftwareBitmap, String> {
    use windows::Storage::Streams::Buffer;
    use windows::Win32::System::WinRT::IBufferByteAccess;
    use windows::core::Interface;

    let mut buffer =
        Buffer::Create(buf.len() as u32).map_err(|e| format!("Buffer 创建失败: {e}"))?;
    buffer
        .SetLength(buf.len() as u32)
        .map_err(|e| format!("Buffer 长度设置失败: {e}"))?;
    {
        let access: IBufferByteAccess = buffer
            .cast()
            .map_err(|e| format!("Buffer 访问接口失败: {e}"))?;
        let ptr = access
            .Buffer()
            .map_err(|e| format!("Buffer 数据获取失败: {e}"))?;
        std::ptr::copy_nonoverlapping(buf.as_ptr(), ptr, buf.len());
    }
    SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        w as i32,
        h as i32,
    )
    .map_err(|e| format!("位图创建失败: {e}"))
}
