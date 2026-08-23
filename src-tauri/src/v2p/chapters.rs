// 视频转 PDF: 机械分章 (按时间等分, 聚合转写片段)
use super::asr::AsrSegment;

#[derive(Clone, Debug)]
pub struct Chapter {
    pub index: usize,
    pub title: String,   // "第 N 章 (MM:SS - MM:SS)"
    pub start: f32,
    pub end: f32,
    pub text: String,    // 该章聚合的转写文本
}

// 按时间等分 segments 为 chapters 章
pub fn split_chapters(segments: &[AsrSegment], chapters: usize) -> Vec<Chapter> {
    if segments.is_empty() {
        return Vec::new();
    }
    let total_end = segments.last().map(|s| s.end).unwrap_or(0.0).max(1.0);
    let seg_len = total_end / chapters as f32;

    let mut out = Vec::new();
    for i in 0..chapters {
        let start = i as f32 * seg_len;
        let end = (i + 1) as f32 * seg_len;
        let text: String = segments
            .iter()
            .filter(|s| s.end > start && s.start < end)
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join("");
        let title = format!("第 {} 章 ({})", i + 1, fmt_range(start, end));
        out.push(Chapter {
            index: i + 1,
            title,
            start,
            end,
            text,
        });
    }
    out
}

fn fmt_range(start: f32, end: f32) -> String {
    format!("{} - {}", fmt_ts(start), fmt_ts(end))
}

pub fn fmt_ts(seconds: f32) -> String {
    let s = seconds.max(0.0) as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_evenly() {
        // 模拟 10 段, 每段 1s, 总时长 10s, 分 3 章
        let segs: Vec<AsrSegment> = (0..10)
            .map(|i| AsrSegment {
                text: format!("句{}", i),
                start: i as f32,
                end: i as f32 + 1.0,
            })
            .collect();
        let ch = split_chapters(&segs, 3);
        assert_eq!(ch.len(), 3);
        // 第1章: 0-3.33s, 应含 句0,句1,句2
        assert!(ch[0].text.contains("句0"));
        assert!(ch[0].text.contains("句2"));
        // 第3章: 6.67-10s, 应含 句7,句8,句9
        assert!(ch[2].text.contains("句8"));
        assert!(ch[2].text.contains("句9"));
        eprintln!("chapters: {:?}", ch.iter().map(|c| c.title.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn empty_segments() {
        let ch = split_chapters(&[], 5);
        assert!(ch.is_empty());
    }
}
