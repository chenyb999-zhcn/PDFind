// 视频转 PDF: 图片文件 OCR (复用 ocr.rs WinRT, 仅 Windows)
#![cfg(windows)]

use std::path::Path;

// 对图片文件执行 OCR, 返回识别文本 (去掉空行)
pub fn ocr_image_file(path: &Path) -> Result<String, String> {
    let img = image::open(path).map_err(|e| format!("图片解码失败: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    // RGBA -> BGRA (winrt ocr 用 Bgra8)
    let mut bgra: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    for p in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[p[2], p[1], p[0], 0xFF]);
    }
    let result = crate::ocr::recognize_bgra(&bgra, w, h)
        .map_err(|e| format!("OCR 失败: {e}"))?;
    // 压缩为单行(去空行)
    let lines: Vec<&str> = result
        .text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(lines.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_frame_image() {
        let path = Path::new(r"C:\msys64\tmp\v2p_frame_test.jpg");
        if !path.exists() {
            eprintln!("skip: 测试帧不存在");
            return;
        }
        let text = ocr_image_file(path).expect("ocr image");
        eprintln!("OCR_TEXT: {}", text);
        assert!(!text.is_empty());
    }
}
