// 视频转 PDF: 计算设备检测
use std::process::Command;

#[derive(Clone, Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub has_nvidia_gpu: bool,
    pub gpu_name: String,
    pub available: Vec<String>, // ["cpu", "cuda"(若有 NVIDIA)]
}

// 检测 NVIDIA GPU (通过 nvidia-smi)
pub fn detect_devices() -> DeviceInfo {
    let has_nvidia = detect_nvidia_smi();
    let gpu_name = if has_nvidia {
        query_gpu_name().unwrap_or_else(|| "NVIDIA GPU".into())
    } else {
        String::new()
    };
    let mut available = vec!["cpu".to_string()];
    if has_nvidia {
        available.push("cuda".to_string());
    }
    DeviceInfo {
        has_nvidia_gpu: has_nvidia,
        gpu_name,
        available,
    }
}

fn nvidia_smi_path() -> Option<String> {
    // 常见路径
    let cands = [
        "nvidia-smi".to_string(),
        r"C:\Windows\System32\nvidia-smi.exe".to_string(),
        r"C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe".to_string(),
    ];
    for c in cands {
        if Command::new(&c).arg("--version").output().is_ok() {
            return Some(c);
        }
    }
    None
}

fn detect_nvidia_smi() -> bool {
    nvidia_smi_path().is_some()
}

fn query_gpu_name() -> Option<String> {
    let exe = nvidia_smi_path()?;
    let out = Command::new(&exe)
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_runs() {
        let d = detect_devices();
        eprintln!("device: {d:?}");
        assert!(d.available.contains(&"cpu".to_string()));
    }
}
