// 视频转 PDF: ASR 转写封装 (sherpa-onnx, FunASR-Nano / Paraformer)
use sherpa_onnx::{
    OfflineFunASRNanoModelConfig, OfflineParaformerModelConfig, OfflineRecognizer,
    OfflineRecognizerConfig, Wave,
};

#[derive(Clone, Debug)]
pub enum AsrEngine {
    FunAsrNano,
    Paraformer,
}

impl AsrEngine {
    pub fn from_str(s: &str) -> Self {
        match s {
            "paraformer" => AsrEngine::Paraformer,
            _ => AsrEngine::FunAsrNano,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AsrSegment {
    pub text: String,
    pub start: f32,
    pub end: f32,
}

pub struct Asr {
    recognizer: OfflineRecognizer,
    engine: AsrEngine,
    // FunASR-Nano max_total_len 限制每段音频长度(秒)
    chunk_seconds: f32,
}

impl Asr {
    pub fn create(engine: AsrEngine, model_dir: &str) -> Result<Self, String> {
        let mut config = OfflineRecognizerConfig::default();
        let chunk_seconds = match &engine {
            AsrEngine::FunAsrNano => {
                config.model_config.funasr_nano = OfflineFunASRNanoModelConfig {
                    encoder_adaptor: Some(format!("{model_dir}\\encoder_adaptor.int8.onnx")),
                    llm: Some(format!("{model_dir}\\llm.int8.onnx")),
                    embedding: Some(format!("{model_dir}\\embedding.int8.onnx")),
                    tokenizer: Some(format!("{model_dir}\\Qwen3-0.6B")),
                    system_prompt: Some("You are a helpful assistant.".into()),
                    user_prompt: Some("语音转写：".into()),
                    max_new_tokens: 512,
                    temperature: 1e-6,
                    top_p: 0.8,
                    seed: 42,
                    language: None,
                    itn: 1,
                    hotwords: None,
                };
                config.model_config.model_type = Some("funasr_nano".into());
                12.0 // FunASR-Nano 每段安全时长
            }
            AsrEngine::Paraformer => {
                config.model_config.paraformer = OfflineParaformerModelConfig {
                    model: Some(format!("{model_dir}\\model.onnx")),
                };
                config.model_config.tokens = Some(format!("{model_dir}\\tokens.txt"));
                config.model_config.model_type = Some("paraformer".into());
                30.0 // Paraformer 支持更长
            }
        };
        config.model_config.num_threads = 4;
        config.model_config.provider = Some("cpu".into());

        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| "创建 ASR 识别器失败".to_string())?;
        Ok(Self {
            recognizer,
            engine,
            chunk_seconds,
        })
    }

    // 转写完整 wav (长音频自动切片), 返回带时间戳的分段
    pub fn transcribe_wav(&self, wav_path: &str) -> Result<Vec<AsrSegment>, String> {
        let wave = Wave::read(wav_path).ok_or_else(|| "读取音频失败".to_string())?;
        self.transcribe_wave(&wave)
    }

    // 转写 Wave 对象 (含切片)
    pub fn transcribe_wave(&self, wave: &Wave) -> Result<Vec<AsrSegment>, String> {
        self.transcribe_wave_with_progress(wave, &mut |_, _| true)
    }

    // 转写 Wave (含切片), 每段完成回调 (done, total_chunks) -> bool (false=取消)
    pub fn transcribe_wave_with_progress(
        &self,
        wave: &Wave,
        on_chunk: &mut dyn FnMut(usize, usize) -> bool,
    ) -> Result<Vec<AsrSegment>, String> {
        let sample_rate = wave.sample_rate() as usize;
        let samples = wave.samples();
        let total = samples.len();
        if total == 0 {
            return Ok(Vec::new());
        }
        let chunk_len = (self.chunk_seconds * sample_rate as f32) as usize;
        // 0.5s 重叠, 避免切词
        let overlap = (sample_rate / 2) as usize;
        let total_chunks = ((total + chunk_len - 1) / chunk_len).max(1);

        let mut all: Vec<AsrSegment> = Vec::new();
        let mut pos = 0usize;
        let mut global_offset = 0.0f32;
        let mut chunk_idx = 0usize;
        while pos < total {
            let end = (pos + chunk_len).min(total);
            let chunk = &samples[pos..end];
            let segments = self.recognize_chunk(chunk, sample_rate)?;
            for mut seg in segments {
                seg.start += global_offset;
                seg.end += global_offset;
                all.push(seg);
            }
            chunk_idx += 1;
            if !on_chunk(chunk_idx, total_chunks) {
                // 取消
                break;
            }
            let advance = end - overlap;
            global_offset += (advance - pos) as f32 / sample_rate as f32;
            if advance <= pos {
                break;
            }
            pos = advance;
        }
        Ok(all)
    }

    // 单段转写 (不含全局偏移)
    fn recognize_chunk(&self, samples: &[f32], sample_rate: usize) -> Result<Vec<AsrSegment>, String> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(sample_rate as i32, samples);
        self.recognizer.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| "转写无结果".to_string())?;

        let timestamps = result.timestamps.clone().unwrap_or_default();
        let tokens = result.tokens.clone();
        if timestamps.is_empty() || tokens.is_empty() {
            return Ok(vec![AsrSegment {
                text: result.text,
                start: 0.0,
                end: samples.len() as f32 / sample_rate as f32,
            }]);
        }

        let mut segments: Vec<AsrSegment> = Vec::new();
        let mut cur_text = String::new();
        let mut cur_start = timestamps[0];
        for (i, tok) in tokens.iter().enumerate() {
            cur_text.push_str(tok);
            let ts = timestamps.get(i).copied().unwrap_or(cur_start);
            let is_end = matches!(
                tok.trim(),
                "。" | "！" | "？" | "," | "." | "!" | "?" | "，"
            );
            if is_end && !cur_text.trim().is_empty() {
                let end = timestamps.get(i + 1).copied().unwrap_or(ts);
                segments.push(AsrSegment {
                    text: cur_text.trim().to_string(),
                    start: cur_start,
                    end,
                });
                cur_text.clear();
                cur_start = end;
            }
        }
        if !cur_text.trim().is_empty() {
            let end = timestamps.last().copied().unwrap_or(cur_start);
            segments.push(AsrSegment {
                text: cur_text.trim().to_string(),
                start: cur_start,
                end,
            });
        }
        if segments.is_empty() {
            segments.push(AsrSegment {
                text: result.text,
                start: 0.0,
                end: samples.len() as f32 / sample_rate as f32,
            });
        }
        Ok(segments)
    }

    pub fn engine(&self) -> &AsrEngine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funasr_nano_transcribes() {
        let model_dir = concat!(env!("CARGO_MANIFEST_DIR"), r"\dev-models\sherpa-onnx-funasr-nano-int8-2025-12-30");
        let wav = format!("{model_dir}\\test_wavs\\dia_hunan.wav");
        if !std::path::Path::new(&wav).exists() {
            eprintln!("skip: 模型/测试音频不存在(未下载)");
            return;
        }
        let asr = Asr::create(AsrEngine::FunAsrNano, model_dir).expect("create asr");
        let segs = asr.transcribe_wav(&wav).expect("transcribe");
        assert!(!segs.is_empty());
        assert!(!segs[0].text.trim().is_empty());
        eprintln!("segs={:?}", segs);
    }

    // 长音频切片转写验证 (真实 mp3 前 45 秒)
    #[test]
    fn long_audio_chunked_transcribe() {
        let model_dir = concat!(env!("CARGO_MANIFEST_DIR"), r"\dev-models\sherpa-onnx-funasr-nano-int8-2025-12-30");
        if !std::path::Path::new(model_dir).join("llm.int8.onnx").exists() {
            eprintln!("skip: 模型未下载");
            return;
        }
        // 45s 音频: 切片逻辑应切成多段(12s/段 + 0.5s 重叠)
        let wav = r"C:\msys64\tmp\pdfind_v2p_45s.wav";
        if !std::path::Path::new(wav).exists() {
            eprintln!("skip: 测试音频不存在");
            return;
        }
        let asr = Asr::create(AsrEngine::FunAsrNano, model_dir).expect("create asr");
        let segs = asr.transcribe_wav(wav).expect("transcribe");
        assert!(!segs.is_empty());
        let total: String = segs.iter().map(|s| s.text.clone()).collect::<Vec<_>>().join("");
        eprintln!("transcribed {} chars, {} segments", total.chars().count(), segs.len());
        eprintln!("first 100: {}", &total.chars().take(100).collect::<String>());
        assert!(!total.trim().is_empty());
    }
}
