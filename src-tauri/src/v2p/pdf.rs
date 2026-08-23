// 视频转 PDF: genpdf 排版 (封面 + 章节 + 图文)
use genpdf::elements::{Break, Image, Paragraph};
use genpdf::fonts::{FontData, FontFamily};
use genpdf::style::Style;
use genpdf::Document;
use std::path::Path;

pub struct PdfSpec {
    pub title: String,
    pub subtitle: String,
    pub meta: Vec<(String, String)>,
    pub chapters: Vec<PdfChapter>,
}

pub struct PdfChapter {
    pub title: String,
    pub text: String,
    pub images: Vec<PdfImage>,
}

pub struct PdfImage {
    pub path: String,
    pub caption: String,
}

fn load_family(font_path: &str) -> Result<FontFamily<FontData>, String> {
    let regular = FontData::load(font_path, None).map_err(|e| format!("加载字体失败: {e}"))?;
    Ok(FontFamily {
        regular: regular.clone(),
        bold: regular.clone(),
        italic: regular.clone(),
        bold_italic: regular,
    })
}

fn heading(text: &str) -> Paragraph {
    let style = Style::new().bold();
    Paragraph::new("").styled_string(text, style)
}

fn body(text: &str) -> Paragraph {
    Paragraph::new(text)
}

pub fn render_pdf(spec: &PdfSpec, font_path: &str, out_pdf: &str) -> Result<(), String> {
    let family = load_family(font_path)?;
    let mut doc = Document::new(family);
    doc.set_title(spec.title.clone());

    // 封面
    doc.push(heading(&spec.title));
    doc.push(Break::new(1));
    if !spec.subtitle.is_empty() {
        doc.push(body(&spec.subtitle));
    }
    for (k, v) in &spec.meta {
        doc.push(body(&format!("{k}: {v}")));
    }
    doc.push(Break::new(2));

    // 章节
    for (i, ch) in spec.chapters.iter().enumerate() {
        doc.push(heading(&format!("{}. {}", i + 1, ch.title)));
        if !ch.text.trim().is_empty() {
            doc.push(body(&ch.text));
        }
        doc.push(Break::new(1));
        for img in &ch.images {
            let p = Path::new(&img.path);
            if p.exists() {
                match Image::from_path(p) {
                    Ok(image) => {
                        let _ = doc.push(image);
                        doc.push(body(&format!("▲ {}", img.caption)));
                    }
                    Err(_) => {}
                }
            }
            doc.push(Break::new(1));
        }
        doc.push(Break::new(2));
    }

    doc.render_to_file(out_pdf)
        .map_err(|e| format!("生成 PDF 失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_sample_pdf() {
        let font = r"C:\Windows\Fonts\Deng.ttf";
        if !Path::new(font).exists() {
            eprintln!("skip: 系统中文字体不存在");
            return;
        }
        let frame = r"C:\msys64\tmp\v2p_frame_test.jpg";
        let spec = PdfSpec {
            title: "测试教材".into(),
            subtitle: "视频转 PDF 验证".into(),
            meta: vec![("时长".into(), "45 秒".into())],
            chapters: vec![
                PdfChapter {
                    title: "第一章 测试".into(),
                    text: "这是第一段正文，用于验证中文排版是否正常显示。".into(),
                    images: vec![PdfImage {
                        path: frame.into(),
                        caption: "测试截图".into(),
                    }],
                },
                PdfChapter {
                    title: "第二章 结尾".into(),
                    text: "第二段正文，验证多章节。".into(),
                    images: vec![],
                },
            ],
        };
        let out = r"C:\msys64\tmp\pdfind_v2p_test.pdf";
        render_pdf(&spec, font, out).expect("render pdf");
        assert!(Path::new(out).exists());
        eprintln!("PDF generated: {out}");
    }
}

