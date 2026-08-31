// 知识库: tauri 命令 (列表/入库/删除/问答/检索/embedding 模型)
use crate::kb::{db, embed, ingest, retrieve};
use crate::v2p::{llm, organizer};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct KbOverview {
    pub docs: Vec<db::DocInfo>,
    pub chunks: i64,
    // embedding 模型是否已下载 (决定能否向量检索)
    pub embed_model: bool,
}

#[tauri::command]
pub fn kb_overview(app: AppHandle) -> Result<KbOverview, String> {
    let d = db::Db::open(&app)?;
    Ok(KbOverview {
        docs: d.list_docs()?,
        chunks: d.count_chunks()?,
        embed_model: embed::embed_model_downloaded(),
    })
}

#[tauri::command]
pub fn kb_remove_doc(app: AppHandle, id: i64) -> Result<(), String> {
    let d = db::Db::open(&app)?;
    d.remove_doc(id)
}

#[tauri::command]
pub async fn kb_add_pdf(app: AppHandle, path: String) -> Result<ingest::IngestResult, String> {
    let d = db::Db::open(&app)?;
    ingest::ingest_pdf(&d, &app, &path, &mut |l| {
        let _ = app.emit("kb:log", l.to_string());
    })
}

#[tauri::command]
pub async fn kb_add_text(app: AppHandle, path: String) -> Result<ingest::IngestResult, String> {
    let d = db::Db::open(&app)?;
    ingest::ingest_text_file(&d, &path, &mut |l| {
        let _ = app.emit("kb:log", l.to_string());
    })
}

#[derive(Serialize, Clone)]
pub struct DlProg {
    pub done: u64,
    pub total: u64,
}

// 下载 embedding 模型 (事件 kb:dl 进度)
#[tauri::command]
pub async fn kb_download_embed_model(app: AppHandle) -> Result<(), String> {
    embed::download_embed_model(&mut |done, total| {
        let _ = app.emit("kb:dl", DlProg { done, total });
    })
}

// ============ 问答 / 检索 ============

fn fmt_ts(s: f64) -> String {
    let t = s.max(0.0) as u64;
    format!("{:02}:{:02}", t / 60, t % 60)
}

// 问题向量 (模型未下载或编码失败 -> None, 退化为纯关键词)
fn question_emb(app: &AppHandle, question: &str) -> Option<Vec<f32>> {
    if !embed::embed_model_downloaded() {
        return None;
    }
    match embed::embed_texts(&[question.to_string()], &mut |l| {
        let _ = app.emit("kb:log", l.to_string());
    }) {
        Ok(v) => v.into_iter().next(),
        Err(e) => {
            let _ = app.emit(
                "kb:log",
                format!("embedding 失败({e}), 退化为关键词检索").to_string(),
            );
            None
        }
    }
}

#[derive(Serialize, Clone)]
pub struct AskSource {
    pub doc_id: i64,
    pub doc_title: String,
    pub chapter: String,
    pub start_s: f64,
    pub end_s: f64,
    pub page: i64,
    pub snippet: String,
}

#[derive(Serialize, Clone)]
pub struct AskResult {
    pub answer: String,
    pub sources: Vec<AskSource>,
}

// RAG 问答: 混合检索 top-8 -> 带编号资料喂给 LLM -> 回答 + 引用来源
#[tauri::command]
pub async fn kb_ask(
    app: AppHandle,
    question: String,
    provider_id: String,
) -> Result<AskResult, String> {
    let d = db::Db::open(&app)?;
    let q_emb = question_emb(&app, &question);
    let hits = retrieve::retrieve(&d, q_emb.as_deref(), &question, 8)?;
    if hits.is_empty() {
        return Err("知识库中没有相关内容".into());
    }

    let mut ctx = String::new();
    for (i, h) in hits.iter().enumerate() {
        let where_s = if h.page > 0 {
            format!("第{}页", h.page)
        } else if h.end_s > 0.0 {
            format!("{}-{}", fmt_ts(h.start_s), fmt_ts(h.end_s))
        } else {
            String::new()
        };
        let chap = if h.chapter.is_empty() {
            String::new()
        } else {
            format!(" [{}]", h.chapter)
        };
        ctx.push_str(&format!(
            "[{}] 《{}》{} {}\n{}\n\n",
            i + 1,
            h.doc_title,
            chap,
            where_s,
            h.text
        ));
    }

    let cfg = organizer::load(&app);
    let (base_url, model, api_key) = llm::resolve_llm(&cfg, &provider_id)?;
    let sys = "你是一个知识库问答助手。请只根据下面提供的资料片段回答用户问题，回答中用 [n] 标注引用了哪个片段。如果资料中没有相关内容，直接回答“资料中没有相关内容”，不要编造。";
    let user = format!("资料：\n{ctx}\n用户问题：{question}");
    let answer = llm::llm_chat(
        &base_url,
        &model,
        &api_key,
        &[
            ("system".to_string(), sys.to_string()),
            ("user".to_string(), user),
        ],
        0.3,
        2048,
        120,
    )?;

    let sources = hits
        .iter()
        .map(|h| AskSource {
            doc_id: h.doc_id,
            doc_title: h.doc_title.clone(),
            chapter: h.chapter.clone(),
            start_s: h.start_s,
            end_s: h.end_s,
            page: h.page,
            snippet: h.text.chars().take(120).collect(),
        })
        .collect();
    Ok(AskResult { answer, sources })
}

// 纯检索 (不调 LLM): 返回混合检索 top-10
#[tauri::command]
pub async fn kb_search(app: AppHandle, question: String) -> Result<Vec<retrieve::Hit>, String> {
    let d = db::Db::open(&app)?;
    let q_emb = question_emb(&app, &question);
    retrieve::retrieve(&d, q_emb.as_deref(), &question, 10)
}
