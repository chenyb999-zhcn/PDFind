// 视频转 PDF: ASR 模型清单与下载元数据
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelFile {
    pub name: String,
    #[serde(default)]
    pub size_mb: u64,
    #[serde(default)]
    pub url_ms: String, // ModelScope 下载地址
    #[serde(default)]
    pub url_hf: String, // HuggingFace 回退
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsrModel {
    pub id: String,
    pub name: String,
    pub runtime: String,      // "llamacpp" | "sherpaonnx"
    #[serde(default)]
    pub cli: String,          // llama.cpp CLI 名(仅 llamacpp)
    #[serde(default)]
    pub gpu: bool,            // 是否支持 CUDA
    #[serde(default)]
    pub needs_vad: bool,
    #[serde(default)]
    pub extract_tar: bool,    // 是否需要解压 tar.bz2
    #[serde(default)]
    pub files: Vec<ModelFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelsCatalog {
    pub version: u64,
    pub models: Vec<AsrModel>,
}

impl ModelsCatalog {
    pub fn find(&self, id: &str) -> Option<&AsrModel> {
        self.models.iter().find(|m| m.id == id)
    }
}

// 远端清单 URL (版本发现用)
pub const REMOTE_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/chenyb999-zhcn/PDFind/main/asr-models.json";

// 内置清单 (本地回退; 远端有新版本时替换)
pub fn builtin_catalog() -> ModelsCatalog {
    ModelsCatalog {
        version: 1,
        models: vec![
            AsrModel {
                id: "nano-llamacpp".into(),
                name: "Fun-ASR-Nano (llama.cpp)".into(),
                runtime: "llamacpp".into(),
                cli: "llama-funasr-cli".into(),
                gpu: true, // CUDA 支持
                needs_vad: true,
                extract_tar: false,
                files: vec![
                    ModelFile {
                        name: "funasr-encoder-f16.gguf".into(),
                        size_mb: 469,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/master/funasr-encoder-f16.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/main/funasr-encoder-f16.gguf".into(),
                    },
                    ModelFile {
                        name: "qwen3-0.6b-q8_0.gguf".into(),
                        size_mb: 805,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/master/qwen3-0.6b-q8_0.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/main/qwen3-0.6b-q8_0.gguf".into(),
                    },
                    ModelFile {
                        name: "fsmn-vad.gguf".into(),
                        size_mb: 1,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/fsmn-vad-GGUF/resolve/master/fsmn-vad.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf".into(),
                    },
                ],
            },
            AsrModel {
                id: "sensevoice-llamacpp".into(),
                name: "SenseVoice-Small (llama.cpp)".into(),
                runtime: "llamacpp".into(),
                cli: "llama-funasr-sensevoice".into(),
                gpu: true, // CUDA 支持
                needs_vad: true,
                extract_tar: false,
                files: vec![
                    ModelFile {
                        name: "sensevoice-small-q8.gguf".into(),
                        size_mb: 170,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/SenseVoiceSmall-GGUF/resolve/master/sensevoice-small-q8.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/SenseVoiceSmall-GGUF/resolve/main/sensevoice-small-q8.gguf".into(),
                    },
                    ModelFile {
                        name: "fsmn-vad.gguf".into(),
                        size_mb: 1,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/fsmn-vad-GGUF/resolve/master/fsmn-vad.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf".into(),
                    },
                ],
            },
            AsrModel {
                id: "paraformer-llamacpp".into(),
                name: "Paraformer (llama.cpp)".into(),
                runtime: "llamacpp".into(),
                cli: "llama-funasr-paraformer".into(),
                gpu: false,
                needs_vad: true,
                extract_tar: false,
                files: vec![
                    ModelFile {
                        name: "paraformer-q8.gguf".into(),
                        size_mb: 130,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/Paraformer-GGUF/resolve/master/paraformer-q8.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/Paraformer-GGUF/resolve/main/paraformer-q8.gguf".into(),
                    },
                    ModelFile {
                        name: "fsmn-vad.gguf".into(),
                        size_mb: 1,
                        url_ms: "https://modelscope.cn/models/FunAudioLLM/fsmn-vad-GGUF/resolve/master/fsmn-vad.gguf".into(),
                        url_hf: "https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf".into(),
                    },
                ],
            },
            AsrModel {
                id: "nano-onnx".into(),
                name: "Fun-ASR-Nano (sherpa-onnx)".into(),
                runtime: "sherpaonnx".into(),
                cli: String::new(),
                gpu: false,
                needs_vad: false,
                extract_tar: false,
                files: vec![
                    ModelFile {
                        name: "encoder_adaptor.int8.onnx".into(),
                        size_mb: 227,
                        url_ms: "https://modelscope.cn/models/csukuangfj/asr-models/resolve/master/sherpa-onnx-funasr-nano-int8-2025-12-30/encoder_adaptor.int8.onnx".into(),
                        url_hf: String::new(),
                    },
                    ModelFile {
                        name: "llm.int8.onnx".into(),
                        size_mb: 573,
                        url_ms: "https://modelscope.cn/models/csukuangfj/asr-models/resolve/master/sherpa-onnx-funasr-nano-int8-2025-12-30/llm.int8.onnx".into(),
                        url_hf: String::new(),
                    },
                    ModelFile {
                        name: "embedding.int8.onnx".into(),
                        size_mb: 149,
                        url_ms: "https://modelscope.cn/models/csukuangfj/asr-models/resolve/master/sherpa-onnx-funasr-nano-int8-2025-12-30/embedding.int8.onnx".into(),
                        url_hf: String::new(),
                    },
                ],
            },
            AsrModel {
                id: "paraformer-onnx".into(),
                name: "Paraformer-zh (sherpa-onnx)".into(),
                runtime: "sherpaonnx".into(),
                cli: String::new(),
                gpu: false,
                needs_vad: false,
                extract_tar: true,
                files: vec![
                    ModelFile {
                        name: "sherpa-onnx-paraformer-zh-2024-03-09.tar.bz2".into(),
                        size_mb: 230,
                        url_ms: "https://modelscope.cn/models/csukuangfj/asr-models/resolve/master/sherpa-onnx-paraformer-zh-2024-03-09.tar.bz2".into(),
                        url_hf: String::new(),
                    },
                ],
            },
            AsrModel {
                id: "whisper".into(),
                name: "Whisper tiny.en (sherpa-onnx)".into(),
                runtime: "sherpaonnx".into(),
                cli: String::new(),
                gpu: false,
                needs_vad: false,
                extract_tar: true,
                files: vec![
                    ModelFile {
                        name: "sherpa-onnx-whisper-tiny.en.tar.bz2".into(),
                        size_mb: 40,
                        url_ms: "https://modelscope.cn/models/csukuangfj/asr-models/resolve/master/sherpa-onnx-whisper-tiny.en.tar.bz2".into(),
                        url_hf: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-tiny.en.tar.bz2".into(),
                    },
                ],
            },
            AsrModel {
                id: "fireredasr".into(),
                name: "FireRedASR2 (sherpa-onnx)".into(),
                runtime: "sherpaonnx".into(),
                cli: String::new(),
                gpu: false,
                needs_vad: false,
                extract_tar: true,
                files: vec![
                    ModelFile {
                        name: "sherpa-onnx-fire-red-asr2-zh_en-int8-2026-02-25.tar.bz2".into(),
                        size_mb: 600,
                        url_ms: "https://modelscope.cn/models/csukuangfj/asr-models/resolve/master/sherpa-onnx-fire-red-asr2-zh_en-int8-2026-02-25.tar.bz2".into(),
                        url_hf: String::new(),
                    },
                ],
            },
        ],
    }
}

// 模型 id -> 完整文件路径 (含目录)
pub fn model_dir() -> std::path::PathBuf {
    // 开发: src-tauri/dev-models; 打包后: 应用数据目录(实施时切换)
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev-models")
}

// 每个模型的本地文件路径: <models_dir>/<engine_id>/<file_name>
pub fn local_paths(model: &AsrModel) -> HashMap<String, std::path::PathBuf> {
    let mut map = HashMap::new();
    for f in &model.files {
        let p = model_dir().join(&model.id).join(&f.name);
        map.insert(f.name.clone(), p);
    }
    map
}

// 模型是否已全部下载
pub fn is_downloaded(model: &AsrModel) -> bool {
    model.files.iter().all(|f| {
        let p = model_dir().join(&model.id).join(&f.name);
        p.exists()
    })
}

// ======================= PDF 整理用在线大模型服务商 (OpenAI 兼容协议) =======================
// API Key 存于后端配置文件, 不随模型下载
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizerProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,          // chat/completions 的 Base URL(不含结尾 /chat/completions)
    pub default_model: String,
    pub needs_model: bool,         // 是否必须用户填 model(如豆包 endpoint id)
    pub models: Vec<String>,       // 预置模型下拉列表
}

pub fn organizer_providers() -> Vec<OrganizerProvider> {
    vec![
        OrganizerProvider {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: "deepseek-chat".into(),
            needs_model: false,
            models: vec![
                "deepseek-chat".into(),
                "deepseek-reasoner".into(),
                "deepseek-v4-flash".into(),
                "deepseek-v4-pro".into(),
            ],
        },
        OrganizerProvider {
            id: "glm".into(),
            name: "智谱 GLM".into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            default_model: "glm-4.7-flash".into(),
            needs_model: false,
            models: vec![
                "glm-5.3".into(),
                "glm-5.2".into(),
                "glm-5.1".into(),
                "glm-5".into(),
                "glm-5-turbo".into(),
                "glm-4.7".into(),
                "glm-4.7-flash".into(),
                "glm-4.6".into(),
                "glm-4-flash-250414".into(),
            ],
        },
        OrganizerProvider {
            id: "kimi".into(),
            name: "Kimi 月之暗面".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            default_model: "kimi-latest".into(),
            needs_model: false,
            models: vec![
                "kimi-latest".into(),
                "kimi-k2.6".into(),
                "kimi-k2.5".into(),
                "moonshot-v1-128k".into(),
                "moonshot-v1-32k".into(),
            ],
        },
        OrganizerProvider {
            id: "doubao".into(),
            name: "豆包 火山方舟".into(),
            base_url: "https://ark.cn-beijing.volces.com/api/v3".into(),
            default_model: String::new(), // 需用户填 Endpoint ID 或模型 ID
            needs_model: true,
            models: vec![
                "doubao-seed-2-1-pro-260628".into(),
                "doubao-seed-2-0-code-preview-260215".into(),
            ],
        },
        OrganizerProvider {
            id: "qwen".into(),
            name: "通义千问".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            default_model: "qwen-plus".into(),
            needs_model: false,
            models: vec![
                "qwen-plus".into(),
                "qwen-max".into(),
                "qwen-turbo".into(),
                "qwen-long".into(),
                "qwen3-max".into(),
            ],
        },
        OrganizerProvider {
            id: "hunyuan".into(),
            name: "腾讯元宝(混元)".into(),
            base_url: "https://api.hunyuan.cloud.tencent.com/v1".into(),
            default_model: "hunyuan-turbos-latest".into(),
            needs_model: false,
            models: vec![
                "hunyuan-turbos-latest".into(),
                "hunyuan-t1-latest".into(),
                "hunyuan-pro".into(),
                "hunyuan-standard".into(),
                "hunyuan-lite".into(),
            ],
        },
        OrganizerProvider {
            id: "custom".into(),
            name: "自定义".into(),
            base_url: String::new(),
            default_model: String::new(),
            needs_model: true,
            models: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_all_models() {
        let cat = builtin_catalog();
        assert!(cat.models.len() >= 7);
        for m in &cat.models {
            assert!(!m.id.is_empty());
            assert!(!m.name.is_empty());
            assert!(!m.files.is_empty(), "{} 无文件", m.id);
            assert!(m.runtime == "llamacpp" || m.runtime == "sherpaonnx");
        }
        for m in cat.models.iter().filter(|m| m.runtime == "llamacpp") {
            assert!(!m.cli.is_empty(), "{} 缺 CLI", m.id);
        }
        let ids: Vec<&str> = cat.models.iter().map(|m| m.id.as_str()).collect();
        let uniq: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), uniq.len(), "模型 id 有重复");
    }

    #[test]
    fn whisper_not_downloaded() {
        let cat = builtin_catalog();
        let m = cat.find("whisper").unwrap();
        assert!(!is_downloaded(m));
    }
}
