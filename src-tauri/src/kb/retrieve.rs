// 知识库: 混合检索 (向量余弦 + FTS trigram, RRF 融合)
use crate::kb::db::Db;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Hit {
    pub chunk_id: i64,
    pub doc_id: i64,
    pub doc_title: String,
    pub chapter: String,
    pub text: String,
    pub start_s: f64,
    pub end_s: f64,
    pub page: i64,
    pub score: f64,
}

const RRF_K: f64 = 60.0;
const CANDIDATES: usize = 30;

// 检索入口: q_emb 为问题的归一化向量(可空 -> 纯关键词)
pub fn retrieve(db: &Db, q_emb: Option<&[f32]>, q_text: &str, top_k: usize) -> Result<Vec<Hit>, String> {
    // 1. 向量检索: 点积 (双方已归一化 = 余弦)
    let mut vec_rank: Vec<(i64, f64)> = Vec::new();
    if let Some(q) = q_emb {
        if !q.is_empty() {
            let rows = db.chunks_with_embedding()?;
            for r in &rows {
                let v = r.embedding.as_ref().unwrap();
                let n = (v.len() / 4).min(q.len());
                if n == 0 {
                    continue;
                }
                let mut dot = 0f64;
                for i in 0..n {
                    let a = f32::from_le_bytes(v[i * 4..i * 4 + 4].try_into().unwrap()) as f64;
                    dot += a * q[i] as f64;
                }
                vec_rank.push((r.id, dot));
            }
            vec_rank.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            vec_rank.truncate(CANDIDATES);
        }
    }

    // 2. 关键词检索 (trigram 需要 >=3 字符)
    let mut fts_rank: Vec<(i64, f64)> = Vec::new();
    if q_text.chars().count() >= 3 {
        fts_rank = db.fts_search(q_text, CANDIDATES)?;
    }

    // 3. RRF 融合: score = Σ 1/(K + rank) (rank 从 1 起)
    let mut scores: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for (rank, (id, _)) in vec_rank.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
    }
    for (rank, (id, _)) in fts_rank.iter().enumerate() {
        *scores.entry(*id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
    }
    if scores.is_empty() {
        return Ok(Vec::new());
    }
    let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(top_k);

    // 4. 取块内容
    let mut out = Vec::new();
    for (id, score) in ranked {
        if let Some(r) = db.chunk_by_id(id) {
            let title = db.doc_title(r.doc_id).unwrap_or_default();
            out.push(Hit {
                chunk_id: r.id,
                doc_id: r.doc_id,
                doc_title: title,
                chapter: r.chapter,
                text: r.text,
                start_s: r.start_s,
                end_s: r.end_s,
                page: r.page,
                score,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RRF 数学验证
    #[test]
    fn rrf_math() {
        // 纯函数验证: 两个榜单 RRF 后, 双榜前二应排最前
        let mut scores: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        for (rank, id) in vec![7i64, 3, 9, 1].into_iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
        for (rank, id) in vec![7i64, 5, 3, 2].into_iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
        let mut v: Vec<_> = scores.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        assert_eq!(v[0].0, 7, "双榜第一应排最前");
        assert!(v[1].0 == 3 || v[1].0 == 5);
    }
}
