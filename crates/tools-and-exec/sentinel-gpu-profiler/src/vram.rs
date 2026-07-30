pub fn detect_vram_gb() -> Option<f64> {
    #[cfg(target_os = "windows")]
    {
        detect_vram_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        detect_vram_unix()
    }
}

#[cfg(target_os = "windows")]
fn detect_vram_windows() -> Option<f64> {
    let output = std::process::Command::new("powershell")
        .args(["-Command", r#"(Get-CimInstance Win32_VideoController).AdapterRAM"#])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        })
        .or_else(|| {
            std::process::Command::new("powershell")
                .args(["-Command", r#"(Get-WmiObject Win32_VideoController).AdapterRAM"#])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                })
        });

    if let Some(val) = output {
        if let Ok(bytes) = val.trim().parse::<f64>() {
            if bytes > 0.0 {
                return Some(bytes / 1_073_741_824.0);
            }
        }
    }

    // Fallback to nvidia-smi
    std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            s.parse::<f64>().ok().map(|mb| mb / 1024.0)
        })
}

#[cfg(not(target_os = "windows"))]
fn detect_vram_unix() -> Option<f64> {
    std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            s.parse::<f64>().ok().map(|mb| mb / 1024.0)
        })
}

pub fn detect_gpu_name() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("powershell")
            .args(["-Command", r#"(Get-CimInstance Win32_VideoController).Name"#])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            })
            .or_else(|| nvidia_smi_name())
    }
    #[cfg(not(target_os = "windows"))]
    {
        nvidia_smi_name()
    }
}

fn nvidia_smi_name() -> Option<String> {
    std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        })
}

pub struct GpuStats {
    pub name: Option<String>,
    pub vram_total_gb: Option<f64>,
    pub vram_used_gb: Option<f64>,
    pub util_gpu: Option<f64>,
    pub temp_c: Option<f64>,
    pub sm_count: Option<u32>,
}

pub fn query_gpu_stats() -> GpuStats {
    let name = detect_gpu_name();
    let vram_total = detect_vram_gb();

    let (vram_used, util_gpu, temp_c) = query_nvidia_smi_stats();
    let sm_count = detect_sm_count();

    GpuStats { name, vram_total_gb: vram_total, vram_used_gb: vram_used, util_gpu, temp_c, sm_count }
}

fn query_nvidia_smi_stats() -> (Option<f64>, Option<f64>, Option<f64>) {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        });

    match out {
        Some(line) => {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            let vram = parts.get(0).and_then(|s| s.parse::<f64>().ok()).map(|mb| mb / 1024.0);
            let util = parts.get(1).and_then(|s| s.parse::<f64>().ok());
            let temp = parts.get(2).and_then(|s| s.parse::<f64>().ok());
            (vram, util, temp)
        }
        None => (None, None, None),
    }
}

fn detect_sm_count() -> Option<u32> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=index", "--format=csv,noheader"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;

    let gpu_count = String::from_utf8_lossy(&out.stdout).lines().count() as u32;
    if gpu_count == 0 { return None; }

    // Rough SM estimates by GPU family from name
    let name = detect_gpu_name()?;
    let name_lower = name.to_lowercase();

    let sm = if name_lower.contains("a100") { 108 }
    else if name_lower.contains("h100") || name_lower.contains("h200") { 132 }
    else if name_lower.contains("v100") { 80 }
    else if name_lower.contains("titan") || name_lower.contains("rtx 30") || name_lower.contains("rtx 40") {
        if name_lower.contains("4090") { 128 }
        else if name_lower.contains("4080") { 76 }
        else if name_lower.contains("4070") { 48 }
        else if name_lower.contains("4060") { 36 }
        else if name_lower.contains("3090") || name_lower.contains("3090 ti") { 82 }
        else if name_lower.contains("3080") { 68 }
        else if name_lower.contains("3070") { 46 }
        else if name_lower.contains("3060") { 28 }
        else { 48 }
    }
    else if name_lower.contains("a10") || name_lower.contains("a16") { 80 }
    else if name_lower.contains("l4") { 60 }
    else if name_lower.contains("l40") || name_lower.contains("l40s") { 108 }
    else { 48 };

    Some(sm * gpu_count)
}
