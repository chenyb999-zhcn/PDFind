// 视频转 PDF: llama.cpp 子进程封装 (Fun-ASR-Nano / SenseVoice / Paraformer GGUF)
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct LlamaCli {
    bin: PathBuf,
    cli_name: String,
}

impl LlamaCli {
    // 查找 CLI 可执行文件; prefer_cuda 时优先 bin-cuda 目录(CUDA 版)
    pub fn find(cli_name: &str, prefer_cuda: bool) -> Option<Self> {
        let exe_name = if cfg!(windows) {
            format!("{cli_name}.exe")
        } else {
            cli_name.to_string()
        };
        let mut cands: Vec<PathBuf> = Vec::new();
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev-models");
        if prefer_cuda {
            cands.push(base.join("bin-cuda").join(&exe_name));
        }
        cands.push(base.join("bin").join(&exe_name));
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                cands.push(dir.join(&exe_name));
                cands.push(dir.join("binaries").join(&exe_name));
            }
        }
        for c in cands {
            if c.is_file() {
                return Some(Self {
                    bin: c,
                    cli_name: cli_name.to_string(),
                });
            }
        }
        None
    }

    // 转写 wav; on_line 每行转写文本回调; on_log CLI 日志(进度)回调; 返回成功与否
    pub fn transcribe(
        &self,
        model_files: &[&str], // 传给 CLI 的模型参数, 如 ["-m","qwen3.gguf","--enc","encoder.gguf"]
        vad_path: Option<&str>,
        wav_path: &str,
        backend: &str, // "cpu" | "cuda"
        lang: &str,    // "zh" | "en" | "ja"
        cancel: Arc<AtomicBool>,
        on_line: &mut dyn FnMut(&str),
        on_log: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        let mut cmd = Command::new(&self.bin);
        cmd.args(model_files);
        cmd.args(["-a", wav_path]);
        if let Some(v) = vad_path {
            cmd.args(["--vad", v]);
        }
        if !lang.is_empty() {
            cmd.args(["--lang", lang]);
        }
        // SenseVoice 支持 --backend; 其他 CLI 若不支持会忽略或报错, 仅对 sensevoice 传
        if self.cli_name.contains("sensevoice") {
            cmd.args(["--backend", backend]);
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let mut child: Child = cmd.spawn().map_err(|e| format!("启动 {} 失败: {e}", self.bin.display()))?;
        let stdout = child.stdout.take().ok_or("无法读取输出")?;
        let stderr = child.stderr.take().ok_or("无法读取错误输出")?;

        // stdout reader 线程: 转写文本行 -> channel (原样)
        const LOG_PFX: &str = "\u{1}LOG\u{1}";
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let log_tx = tx.clone();
        let reader_thread = thread::spawn(move || {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let t = line.trim().to_string();
                        if !t.is_empty() {
                            let _ = tx.send(t);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // stderr reader 线程: CLI 日志(带 [vad]/[encoder]/进度/done), 加标记后进同一 channel
        let stderr_thread = thread::spawn(move || {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let t = line.trim().to_string();
                        if !t.is_empty() {
                            let mut msg = String::from(LOG_PFX);
                            msg.push_str(&t);
                            let _ = log_tx.send(msg);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // 主线程: 轮询子进程结束 + 取消 + 处理收到的行
        loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("已取消".into());
            }
            // 非阻塞收取已有行
            while let Ok(line) = rx.try_recv() {
                if let Some(rest) = line.strip_prefix("\u{1}LOG\u{1}") {
                    on_log(rest);
                } else {
                    on_line(&line);
                }
            }
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => thread::sleep(std::time::Duration::from_millis(50)),
                Err(e) => {
                    let _ = child.kill();
                    return Err(format!("进程出错: {e}"));
                }
            }
        }
        // 收尾: 处理残余行
        while let Ok(line) = rx.try_recv() {
            if let Some(rest) = line.strip_prefix("\u{1}LOG\u{1}") {
                on_log(rest);
            } else {
                on_line(&line);
            }
        }
        let _ = reader_thread.join();
        let _ = stderr_thread.join();
        Ok(())
    }

    pub fn bin_path(&self) -> &Path {
        &self.bin
    }
}
