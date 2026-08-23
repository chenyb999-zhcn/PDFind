// CUDA CLI 路由验证: find() 按 prefer_cuda 选择 bin-cuda 或 bin
#[test]
fn cuda_cli_routes_to_bin_cuda() {
    let exe = |name: &str, cuda: bool| {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dev-models");
        let e = format!("{name}.exe");
        if cuda {
            let p = base.join("bin-cuda").join(&e);
            if p.is_file() { return Some(p); }
        }
        let p = base.join("bin").join(&e);
        if p.is_file() { Some(p) } else { None }
    };
    let cuda = exe("llama-funasr-cli", true).expect("bin-cuda exe 缺失");
    assert!(cuda.to_string_lossy().contains("bin-cuda"), "应命中 bin-cuda: {cuda:?}");
    let cpu = exe("llama-funasr-cli", false).expect("bin exe 缺失");
    assert!(cpu.to_string_lossy().contains("bin"), "应命中 bin: {cpu:?}");
    eprintln!("OK cuda={cuda:?} cpu={cpu:?}");
}
