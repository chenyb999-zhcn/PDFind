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

// ======================= PDF 整理用 LLM 模型 (与 ASR 模型分离) =======================
// 存于 <models_dir>/organizers/<id>/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizerModel {
    pub id: String,
    pub name: String,
    pub file: ModelFile, // 单个 gguf
}

pub fn organizer_dir() -> std::path::PathBuf {
    model_dir().join("organizers")
}

pub fn organizer_models() -> Vec<OrganizerModel> {
    vec![
        OrganizerModel {
            id: "qwen3-1.7b".into(),
            name: "Qwen3-1.7B (Q8_0, 1.8GB)".into(),
            file: ModelFile {
                name: "Qwen3-1.7B-Q8_0.gguf".into(),
                size_mb: 1834,
                url_ms: "https://modelscope.cn/models/Qwen/Qwen3-1.7B-GGUF/resolve/master/Qwen3-1.7B-Q8_0.gguf".into(),
                url_hf: "https://huggingface.co/Qwen/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q8_0.gguf".into(),
            },
        },
        OrganizerModel {
            id: "qwen3.8-2b".into(),
            name: "Qwen3.8-2B (Q4_K_M, 1.3GB)".into(),
            file: ModelFile {
                name: "Qwen3.8-2B-Q4_K_M.gguf".into(),
                size_mb: 1312,
                url_ms: "https://modelscope.cn/models/empero-ai/Qwen3.8-2B-Distill-GGUF/resolve/master/Qwen3.8-2B-Q4_K_M.gguf".into(),
                url_hf: "https://huggingface.co/empero-ai/Qwen3.8-2B-Distill-GGUF/resolve/main/Qwen3.8-2B-Q4_K_M.gguf".into(),
            },
        },
        OrganizerModel {
            id: "qwen3.8-4b".into(),
            name: "Qwen3.8-4B (Q4_K_M, 2.6GB)".into(),
            file: ModelFile {
                name: "Qwen3.8-4B-Q4_K_M.gguf".into(),
                size_mb: 2783,
                url_ms: "https://modelscope.cn/models/empero-ai/Qwen3.8-4B-Distill-GGUF/resolve/master/Qwen3.8-4B-Q4_K_M.gguf".into(),
                url_hf: "https://huggingface.co/empero-ai/Qwen3.8-4B-Distill-GGUF/resolve/main/Qwen3.8-4B-Q4_K_M.gguf".into(),
            },
        },
    ]
}

pub fn organizer_path(m: &OrganizerModel) -> std::path::PathBuf {
    organizer_dir().join(&m.id).join(&m.file.name)
}

pub fn organizer_downloaded(m: &OrganizerModel) -> bool {
    organizer_path(m).exists()
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
