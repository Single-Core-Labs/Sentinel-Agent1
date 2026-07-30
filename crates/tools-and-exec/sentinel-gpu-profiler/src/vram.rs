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

pub struct ExtendedGpuInfo {
    pub name: Option<String>,
    pub vram_total_gb: Option<f64>,
    pub vram_used_gb: Option<f64>,
    pub util_gpu: Option<f64>,
    pub temp_c: Option<f64>,
    pub sm_count: Option<u32>,
    pub driver_version: Option<String>,
    pub cuda_version: Option<String>,
    pub compute_capability: Option<String>,
    pub core_clock_mhz: Option<f64>,
    pub mem_clock_mhz: Option<f64>,
    pub power_w: Option<f64>,
    pub pcie_gen: Option<String>,
    pub pcie_width: Option<String>,
}

pub fn query_extended_gpu_info() -> ExtendedGpuInfo {
    let name = detect_gpu_name();
    let vram_total = detect_vram_gb();
    let (vram_used, util_gpu, temp_c) = query_nvidia_smi_stats();
    let sm_count = detect_sm_count();

    // Parse extended info from nvidia-smi -q
    let extended = parse_nvidia_smi_query();

    ExtendedGpuInfo {
        name,
        vram_total_gb: vram_total,
        vram_used_gb: vram_used,
        util_gpu,
        temp_c,
        sm_count,
        driver_version: extended.get("Driver Version").cloned(),
        cuda_version: extended.get("CUDA Version").cloned(),
        compute_capability: None,
        core_clock_mhz: extended.get("Max Clocks").and_then(|v| {
            v.lines().find(|l| l.contains("SM"))
                .and_then(|l| l.split(':').nth(1).and_then(|s| s.trim().trim_end_matches(" MHz").parse::<f64>().ok()))
        }),
        mem_clock_mhz: extended.get("Max Clocks").and_then(|v| {
            v.lines().find(|l| l.contains("Memory"))
                .and_then(|l| l.split(':').nth(1).and_then(|s| s.trim().trim_end_matches(" MHz").parse::<f64>().ok()))
        }),
        power_w: extended.get("Power Draw").and_then(|v| {
            v.trim_end_matches(" W").trim().parse::<f64>().ok()
        }),
        pcie_gen: extended.get("Current PCIe").and_then(|v| {
            v.lines().next().map(|l| l.trim().to_string())
        }),
        pcie_width: extended.get("Current PCIe").and_then(|v| {
            v.lines().nth(1).map(|l| l.trim().to_string())
        }),
    }
}

fn parse_nvidia_smi_query() -> std::collections::HashMap<String, String> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["-q"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string());

    let mut map = std::collections::HashMap::new();
    if let Some(text) = out {
        let mut current_section = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with(':') {
                current_section = trimmed.trim_end_matches(':').to_string();
            } else if let Some((key, value)) = trimmed.split_once(':') {
                let k = key.trim();
                let v = value.trim();
                if current_section.is_empty() {
                    map.insert(k.to_string(), v.to_string());
                } else {
                    let section_key = format!("{}: {}", current_section, k);
                    map.entry(section_key.clone()).or_insert_with(|| String::new());
                    if let Some(existing) = map.get_mut(&section_key) {
                        if !existing.is_empty() { existing.push('\n'); }
                        existing.push_str(v);
                    }
                }
            }
        }
    }
    map
}

pub fn compute_capability_from_name(name: &str) -> Option<String> {
    let n = name.to_lowercase();
    if n.contains("h100") || n.contains("h200") { Some("9.0".into()) }
    else if n.contains("a100") || n.contains("a30") { Some("8.0".into()) }
    else if n.contains("a10") || n.contains("a16") { Some("8.6".into()) }
    else if n.contains("l40") || n.contains("l4") { Some("8.9".into()) }
    else if n.contains("rtx 4090") || n.contains("rtx 4080") || n.contains("rtx 4070") {
        if n.contains("4090") || n.contains("4080") { Some("8.9".into()) }
        else { Some("8.9".into()) }
    }
    else if n.contains("rtx 4060") { Some("8.9".into()) }
    else if n.contains("rtx 3090") || n.contains("rtx 3080") || n.contains("rtx 3070") || n.contains("rtx 3060") {
        Some("8.6".into())
    }
    else if n.contains("titan") && (n.contains("v") || n.contains("rtx")) { Some("7.5".into()) }
    else if n.contains("v100") { Some("7.0".into()) }
    else if n.contains("t4") { Some("7.5".into()) }
    else if n.contains("p100") || n.contains("p40") { Some("6.0".into()) }
    else if n.contains("k80") || n.contains("k40") { Some("3.7".into()) }
    else { None }
}

pub fn architecture_from_name(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    if n.contains("h100") || n.contains("h200") { Some("Hopper") }
    else if n.contains("a100") || n.contains("a30") { Some("Ampere") }
    else if n.contains("a10") || n.contains("a16") { Some("Ampere") }
    else if n.contains("l40") || n.contains("l4") { Some("Ada Lovelace") }
    else if n.contains("rtx 40") { Some("Ada Lovelace") }
    else if n.contains("rtx 30") { Some("Ampere") }
    else if n.contains("rtx 20") { Some("Turing") }
    else if n.contains("titan") { Some("Turing/Volta") }
    else if n.contains("v100") { Some("Volta") }
    else if n.contains("t4") { Some("Turing") }
    else if n.contains("p100") || n.contains("p40") { Some("Pascal") }
    else { None }
}

pub fn nvcc_arch_flag(cc: &str) -> &'static str {
    match cc {
        "9.0" => "-arch=sm_90",
        "8.9" => "-arch=sm_89",
        "8.6" => "-arch=sm_86",
        "8.0" => "-arch=sm_80",
        "7.5" => "-arch=sm_75",
        "7.0" => "-arch=sm_70",
        "6.0" => "-arch=sm_60",
        "3.7" => "-arch=sm_37",
        _ => "-arch=sm_86",
    }
}

pub fn gpu_context_string(name: Option<&str>, vram_gb: Option<f64>, extended: &ExtendedGpuInfo) -> String {
    let name = name.unwrap_or("No GPU detected");
    let vram = vram_gb.map(|v| format!("{:.1} GB", v)).unwrap_or_else(|| "N/A".into());
    let arch = architecture_from_name(name).unwrap_or("Unknown");
    let cc_str = extended.compute_capability.clone()
        .or_else(|| compute_capability_from_name(name));
    let cc = cc_str.as_deref().unwrap_or("Unknown");
    let sm = extended.sm_count.map(|s| s.to_string()).unwrap_or_else(|| "Unknown".into());
    let driver = extended.driver_version.as_deref().unwrap_or("Unknown");
    let cuda_v = extended.cuda_version.as_deref().unwrap_or("Unknown");
    let util = extended.util_gpu.map(|u| format!("{:.0}%", u)).unwrap_or_else(|| "N/A".into());
    let temp = extended.temp_c.map(|t| format!("{:.0}°C", t)).unwrap_or_else(|| "N/A".into());
    let mem_clock = extended.mem_clock_mhz.map(|m| format!("{:.0} MHz", m)).unwrap_or_else(|| "N/A".into());
    let nvcc_flag = nvcc_arch_flag(cc);

    format!(
        "GPU: {} | VRAM: {} | Arch: {} | Compute: {} | {} SMs\n\
         Driver: {} | CUDA: {} | Temp: {} | Util: {}\n\
         Mem Clock: {} | NVCC: {}\n\
         CUDA Compatible: sm_50 sm_52 sm_60 sm_61 sm_70 sm_75 sm_80 sm_86 sm_89 sm_90",
        name, vram, arch, cc, sm, driver, cuda_v, temp, util, mem_clock, nvcc_flag
    )
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
