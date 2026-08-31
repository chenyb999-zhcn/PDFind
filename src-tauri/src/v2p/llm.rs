// 通用 OpenAI 兼容 chat/completions 调用 (v2p 整理 + kb 问答共用)
use crate::v2p::{models, organizer};

// 解析服务商配置 -> (base_url, model, api_key); provider_id 可为 "custom"
pub fn resolve_llm(
    cfg: &organizer::OrganizerConfig,
    provider_id: &str,
) -> Result<(String, String, String), String> {
    if provider_id == "custom" {
        let c = &cfg.custom;
        if c.base_url.is_empty() || c.model.is_empty() || c.api_key.is_empty() {
            return Err("自定义服务商需填 Base URL / Model / API Key".into());
        }
        return Ok((c.base_url.clone(), c.model.clone(), c.api_key.clone()));
    }
    let p = models::organizer_providers()
        .into_iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| format!("未知服务商: {provider_id}"))?;
    let key = cfg.keys.get(provider_id).cloned().unwrap_or_default();
    if key.is_empty() {
        return Err(format!("{} 的 API Key 未配置", p.name));
    }
    let model = cfg
        .models
        .get(provider_id)
        .cloned()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| p.default_model.clone());
    if model.is_empty() {
        return Err(format!("{} 需填写 Model(Endpoint ID)", p.name));
    }
    Ok((p.base_url.clone(), model, key))
}

// 一次 chat 调用 (one-shot, 非流式); 返回 assistant 文本
pub fn llm_chat(
    base_url: &str,
    model: &str,
    api_key: &str,
    messages: &[(String, String)], // (role, content)
    temperature: f64,
    max_tokens: u32,
    timeout_secs: u64,
) -> Result<String, String> {
    if base_url.is_empty() {
        return Err("Base URL 为空".into());
    }
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|(r, c)| serde_json::json!({"role": r, "content": c}))
        .collect();
    let body = serde_json::json!({
        "model": model,
        "messages": msgs,
        "temperature": temperature,
        "max_tokens": max_tokens
    });
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("LLM 请求失败: {e}"))?;
    let status = resp.status();
    if status != 200 {
        let body_txt = resp.into_string().unwrap_or_default();
        return Err(format!(
            "LLM 返回 HTTP {status}: {}",
            &body_txt.chars().take(300).collect::<String>()
        ));
    }
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("解析响应失败: {e}"))?;
    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or("LLM 响应无内容")?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("LLM 无输出".into());
    }
    Ok(text)
}
