// 知识库: 文本分块 (~400 字, 优先在句边界切分)
use crate::v2p::commands::SegmentInfo;

pub struct Chunk {
    pub text: String,
    pub chapter: String,
    pub start_s: f64,
    pub end_s: f64,
    pub page: u32,
}

const TARGET: usize = 400; // 目标块长(字符)
const MAX: usize = 560; // 硬上限

// 句结束符
fn is_sent_end(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '；' | '\n' | '.' | '!' | '?' | ';' | '…')
}

// 把一段连续文本按 ~TARGET 字切成若干块(优先句边界, 超 MAX 硬切)
fn split_long(text: &str) -> Vec<String> {
    if text.chars().count() <= TARGET {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + MAX).min(chars.len());
        // 在 [TARGET..MAX) 范围内找最后一个句边界
        let mut cut = None;
        for (i, c) in chars.iter().enumerate().take(end).skip(start + TARGET) {
            if is_sent_end(*c) {
                cut = Some(i + 1);
            }
        }
        let seg_end = cut.unwrap_or(end).min(end);
        out.push(chars[start..seg_end].iter().collect());
        start = seg_end;
    }
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

// 视频: 按 segments 聚块 (保留时间戳), 块间尽量对齐 TARGET 字
pub fn chunk_segments(segs: &[SegmentInfo]) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut cur: Vec<&SegmentInfo> = Vec::new();
    let mut cur_len = 0usize;
    for s in segs {
        cur.push(s);
        cur_len += s.text.chars().count();
        if cur_len >= TARGET && cur.last().map(|x| is_sent_end(x.text.chars().next_back().unwrap_or(' '))).unwrap_or(false) {
            flush(&mut out, &mut cur, &mut cur_len);
        }
    }
    flush(&mut out, &mut cur, &mut cur_len);
    out
}

fn flush(out: &mut Vec<Chunk>, cur: &mut Vec<&SegmentInfo>, cur_len: &mut usize) {
    if cur.is_empty() {
        return;
    }
    let text: String = cur.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
    let start = cur.first().unwrap().start as f64;
    let end = cur.last().unwrap().end as f64;
    let mut parts = split_long(&text);
    if parts.len() == 1 {
        out.push(Chunk { text: parts.pop().unwrap(), chapter: String::new(), start_s: start, end_s: end, page: 0 });
    } else {
        // 多块时按字符比例粗略分摊时间戳
        let total: usize = parts.iter().map(|p| p.chars().count()).sum();
        let mut off = 0usize;
        for p in parts {
            let n = p.chars().count();
            let frac0 = if total > 0 { off as f64 / total as f64 } else { 0.0 };
            let frac1 = if total > 0 { (off + n) as f64 / total as f64 } else { 1.0 };
            out.push(Chunk { text: p, chapter: String::new(), start_s: start + (end - start) * frac0, end_s: start + (end - start) * frac1, page: 0 });
            off += n;
        }
    }
    cur.clear();
    *cur_len = 0;
}

// PDF: 按页文本分块 (保留页码)
pub fn chunk_page_texts(pages: &[String]) -> Vec<Chunk> {
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let t = page.trim();
        if t.is_empty() {
            continue;
        }
        for part in split_long(t) {
            out.push(Chunk { text: part, chapter: String::new(), start_s: 0.0, end_s: 0.0, page: (i + 1) as u32 });
        }
    }
    out
}

// 纯文本: 先按段落聚合, 再按 TARGET 切
pub fn chunk_plain_text(text: &str) -> Vec<Chunk> {
    let replaced = text.replace('\r', "\n");
    let t = replaced.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // 段落级缓冲
    let mut buf = String::new();
    for para in t.split('\n') {
        let p = para.trim();
        if p.is_empty() {
            if buf.chars().count() > 0 {
                for part in split_long(&buf) {
                    out.push(Chunk { text: part, chapter: String::new(), start_s: 0.0, end_s: 0.0, page: 0 });
                }
                buf.clear();
            }
            continue;
        }
        if buf.chars().count() + p.chars().count() > TARGET && !buf.is_empty() {
            for part in split_long(&buf) {
                out.push(Chunk { text: part, chapter: String::new(), start_s: 0.0, end_s: 0.0, page: 0 });
            }
            buf.clear();
        }
        buf.push_str(p);
        buf.push(' ');
    }
    if !buf.trim().is_empty() {
        for part in split_long(&buf) {
            out.push(Chunk { text: part, chapter: String::new(), start_s: 0.0, end_s: 0.0, page: 0 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t: &str, s: f32, e: f32) -> SegmentInfo {
        SegmentInfo { text: t.into(), start: s, end: e }
    }

    #[test]
    fn segments_grouping() {
        let segs: Vec<SegmentInfo> = (0..120)
            .map(|i| seg(&format!("这是第{}句测试内容。", i), i as f32 * 2.0, i as f32 * 2.0 + 2.0))
            .collect();
        let chunks = chunk_segments(&segs);
        // 120 句 ~1680 字, 目标 400 字/块 -> 约 4-5 块
        assert!(chunks.len() >= 3, "应切成多块, got {}", chunks.len());
        for c in &chunks {
            assert!(!c.text.is_empty());
            assert!(c.end_s >= c.start_s);
        }
        // 时间戳单调
        for w in chunks.windows(2) {
            assert!(w[0].start_s <= w[1].start_s);
        }
        // 内容不丢
        let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        let expect: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, expect);
    }

    #[test]
    fn page_chunking_keeps_page() {
        let long: String = "中文测试句子。".repeat(100);
        let pages = vec![String::new(), long.clone()];
        let chunks = chunk_page_texts(&pages);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.page == 2));
    }

    #[test]
    fn plain_text_empty() {
        assert!(chunk_plain_text("   \n  ").is_empty());
    }

    #[test]
    fn plain_text_splits() {
        let t: String = "这是一段比较长的测试文本，用于验证分块逻辑是否正确工作。".repeat(80);
        let chunks = chunk_plain_text(&t);
        assert!(chunks.len() >= 3, "应切成多块, got {}", chunks.len());
        let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(joined.contains("验证分块逻辑"));
    }
}
