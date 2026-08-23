// 端到端: bin-cuda(CUDA) 转写 45s 音频, 模拟前端 device=cuda
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn nano_cuda_end_to_end() {
    let wav = r"C:\msys64\tmp\pdfind_v2p_45s.wav";
    if !std::path::Path::new(wav).exists() { eprintln!("skip: 无测试音频"); return; }
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev-models");
    let cli_bin = base.join("bin-cuda").join("llama-funasr-cli.exe");
    assert!(cli_bin.is_file(), "缺少 bin-cuda/llama-funasr-cli.exe");
    let mdir = base.join("nano-llamacpp");
    let enc = mdir.join("funasr-encoder-f16.gguf");
    let llm = mdir.join("qwen3-0.6b-q8_0.gguf");
    let vad = mdir.join("fsmn-vad.gguf");
    for p in [&enc, &llm, &vad] { assert!(p.is_file(), "缺少 {}", p.display()); }
    // 用原生 Command 跑(不依赖 llamacpp crate 内部, 但模拟同样参数)
    let mut cmd = std::process::Command::new(&cli_bin);
    cmd.arg("--enc").arg(&enc);
    cmd.arg("-m").arg(&llm);
    cmd.arg("-a").arg(wav);
    cmd.arg("--vad").arg(&vad);
    let out = cmd.output().expect("run cli");
    assert!(out.status.success(), "cli 失败 {:?}", String::from_utf8_lossy(&out.stderr).chars().take(500).collect::<String>());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.trim().is_empty(), "转写为空");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("CUDA backend"), "未用 CUDA: {}", stderr.chars().take(200).collect::<String>());
    if let Some(pos) = stderr.find("[done]") { eprintln!("DONE {}", stderr[pos..].lines().next().unwrap_or("")); }
    eprintln!("text[:80]={}", text.chars().take(80).collect::<String>());
}
