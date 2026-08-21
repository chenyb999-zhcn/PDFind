// 搜索匹配器:固定串/正则 + 大小写/整词选项
use regex::{Regex, RegexBuilder};

pub struct Matcher {
    pub re: Regex,
}

impl Matcher {
    // regex_mode=false 时对固定串转义; whole_word 用 \b 包裹
    pub fn new(
        pattern: &str,
        regex_mode: bool,
        case_insensitive: bool,
        whole_word: bool,
    ) -> Result<Self, String> {
        if pattern.is_empty() {
            return Err("搜索词为空".into());
        }
        let src = if regex_mode {
            pattern.to_string()
        } else {
            regex::escape(pattern)
        };
        let src = if whole_word {
            format!(r"\b(?:{})\b", src)
        } else {
            src
        };
        let mut b = RegexBuilder::new(&src);
        b.case_insensitive(case_insensitive);
        let re = b.build().map_err(|e| format!("正则表达式错误: {e}"))?;
        Ok(Self { re })
    }
}
