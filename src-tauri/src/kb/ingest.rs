// 知识库: 入库 (视频/PDF/文本) -> 文档 + 分块 + 本地 embedding
use crate::kb::{chunking, db, embed};
use crate::v2p::commands::SegmentInfo;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone)]
pub struct IngestResult {
    pub doc_id: i64,
    pub chunks: usize,
    // 是否带向量 (embedding 模型未下载时 false, 检索退化为纯关键词)
    pub embedded: bool,
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

// 入库主流程: 同来源覆盖 + 分块 + embedding + 写库
fn write_doc(
    db: &db::Db,
    kind: &str,
    title: &str,
    source_path: &str,
    pdf_path: Option<&str>,
    lang: &str,
    duration_s: f64,
    meta: &str,
    chunks: Vec<chunking::Chunk>,
    on_log: &mut dyn FnMut(&str),
) -> Result<IngestResult, String> {
    if chunks.is_empty() {
        return Err("没有可入库的文本内容".into());
    }
    // 同来源覆盖: 删旧文档 (块经外键级联删除)
    if let Some(old) = db.find_doc_by_source(kind, source_path)? {
        db.remove_doc(old)?;
    }
    let doc_id = db.insert_doc(kind, title, source_path, pdf_path, lang, duration_s, meta)?;

    // 批量 embedding (模型未下载则退化为纯关键词模式)
    let embedded = embed::embed_model_downloaded();
    let embs = if embedded {
        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        on_log(&format!("编码 {} 个文本块…", texts.len()));
        Some(embed::embed_texts(&texts, on_log)?)
    } else {
        None
    };

    for (i, c) in chunks.iter().enumerate() {
        let blob = embs.as_ref().map(|v| to_bytes(&v[i]));
        db.insert_chunk(
            doc_id,
            i as i64,
            &c.chapter,
            &c.text,
            c.start_s,
            c.end_s,
            c.page as i64,
            blob.as_deref(),
        )?;
    }
    db.set_doc_chunks(doc_id, chunks.len() as i64)?;
    Ok(IngestResult {
        doc_id,
        chunks: chunks.len(),
        embedded,
    })
}

// 视频: 按 segments 聚块(带时间戳); outline_md 为 LLM 整理的大纲(仅存 meta 展示)
pub fn ingest_video(
    db: &db::Db,
    source_path: &str,
    pdf_path: Option<&str>,
    lang: &str,
    segments: &[SegmentInfo],
    outline_md: Option<&str>,
    on_log: &mut dyn FnMut(&str),
) -> Result<IngestResult, String> {
    let duration = segments.iter().map(|s| s.end as f64).fold(0.0f64, f64::max);
    let meta = match outline_md {
        Some(md) => serde_json::json!({ "outline": md }).to_string(),
        None => "{}".into(),
    };
    let chunks = chunking::chunk_segments(segments);
    write_doc(
        db,
        "video",
        &stem(source_path),
        source_path,
        pdf_path,
        lang,
        duration,
        &meta,
        chunks,
        on_log,
    )
}

// PDF: pdfium 抽页文本 -> 按页分块(带页码)
pub fn ingest_pdf(
    db: &db::Db,
    app: &tauri::AppHandle,
    path: &str,
    on_log: &mut dyn FnMut(&str),
) -> Result<IngestResult, String> {
    let pdfium = crate::pdfx::get(app)?;
    let pages = crate::pdfx::extract_pages(pdfium, Path::new(path), None)
        .map_err(|e| format!("读取 PDF 失败: {e}"))?;
    if pages.is_empty() {
        return Err("PDF 无页面".into());
    }
    let chunks = chunking::chunk_page_texts(&pages);
    write_doc(db, "pdf", &stem(path), path, None, "zh", 0.0, "{}", chunks, on_log)
}

// 纯文本文件 (utf-8)
pub fn ingest_text_file(
    db: &db::Db,
    path: &str,
    on_log: &mut dyn FnMut(&str),
) -> Result<IngestResult, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("读取文件失败: {e}"))?;
    let chunks = chunking::chunk_plain_text(&content);
    write_doc(db, "text", &stem(path), path, None, "zh", 0.0, "{}", chunks, on_log)
}
