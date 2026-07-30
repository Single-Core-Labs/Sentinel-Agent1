use anyhow::Result;
use std::process::Command as PCommand;

#[derive(Default)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub memory_gb: f64,
    pub gpu: Option<String>,
    pub has_ollama: bool,
}

pub fn detect_system() -> SystemInfo {
    let os = if cfg!(target_os = "windows") {
        "Windows".into()
    } else if cfg!(target_os = "macos") {
        "macOS".into()
    } else {
        "Linux".into()
    };

    let arch = std::env::consts::ARCH.to_string();
    let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);

    let memory_gb = sysinfo_gb();
    let gpu = detect_gpu();
    let has_ollama = which("ollama");

    SystemInfo { os, arch, cpu_cores, memory_gb, gpu, has_ollama }
}

pub fn detect_gpu() -> Option<String> {
    if cfg!(target_os = "windows") {
        run_cmd("wmic", &["path", "win32_VideoController", "get", "name"])
            .or_else(|| run_cmd("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"]))
    } else if cfg!(target_os = "macos") {
        run_cmd("system_profiler", &["SPDisplaysDataType"])
            .map(|s| {
                s.lines()
                    .find(|l| l.contains("Chipset Model") || l.contains("Metal"))
                    .unwrap_or("Apple Silicon")
                    .to_string()
            })
    } else {
        run_cmd("nvidia-smi", &["--query-gpu=name", "--format=csv,noheader"])
            .or_else(|| run_cmd("rocminfo", &[]).map(|_| "AMD GPU detected".into()))
            .or_else(|| Some("Unknown".into()))
    }
}

pub fn install_ollama() -> Result<String> {
    if cfg!(target_os = "windows") {
        install_ollama_windows()
    } else if cfg!(target_os = "macos") {
        install_ollama_macos()
    } else {
        install_ollama_linux()
    }
}

fn install_ollama_windows() -> Result<String> {
    let url = "https://ollama.com/download/OllamaSetup.exe";
    let exe_path = std::env::temp_dir().join("OllamaSetup.exe");

    download_file(url, &exe_path)?;

    let status = PCommand::new(&exe_path)
        .arg("/verysilent")
        .status()
        .map_err(|e| anyhow::anyhow!("Install failed: {}", e))?;

    if status.success() {
        std::fs::remove_file(&exe_path).ok();
        Ok("Ollama installed. Start it from Start Menu or run `ollama serve`.".into())
    } else {
        anyhow::bail!("Ollama installer exited with code {:?}", status.code());
    }
}

fn install_ollama_macos() -> Result<String> {
    let output = PCommand::new("sh")
        .args(["-c", "curl -fsSL https://ollama.com/install.sh | sh"])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run Ollama install script: {}", e))?;

    if output.status.success() {
        Ok("Ollama installed via Homebrew.".into())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already installed") || stderr.contains("up-to-date") {
            Ok("Ollama already installed.".into())
        } else {
            anyhow::bail!("Install failed: {}", stderr);
        }
    }
}

fn install_ollama_linux() -> Result<String> {
    let output = PCommand::new("sh")
        .args(["-c", "curl -fsSL https://ollama.com/install.sh | sh"])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run Ollama install script: {}", e))?;

    if output.status.success() {
        Ok("Ollama installed.".into())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already installed") {
            Ok("Ollama already installed.".into())
        } else {
            anyhow::bail!("Install failed: {}", stderr);
        }
    }
}

pub fn pull_model(model: &str) -> Result<String> {
    let output = PCommand::new("ollama")
        .args(["pull", model])
        .output()
        .map_err(|e| anyhow::anyhow!("ollama pull failed: {}", e))?;

    if output.status.success() {
        Ok(format!("Model `{}` pulled successfully.", model))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ollama pull failed: {}", stderr.trim());
    }
}

pub fn list_local_models() -> Result<Vec<String>> {
    let output = PCommand::new("ollama")
        .args(["list"])
        .output()
        .map_err(|e| anyhow::anyhow!("ollama list failed: {}", e))?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let models: Vec<String> = stdout
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
        .collect();
    Ok(models)
}

pub fn ensure_ollama_running() -> Result<()> {
    if ollama_ping().is_ok() {
        return Ok(());
    }
    start_oll_background()?;
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if ollama_ping().is_ok() {
            return Ok(());
        }
    }
    anyhow::bail!("Ollama failed to start within 30s. Run `ollama serve` manually.");
}

fn ollama_ping() -> Result<()> {
    let resp = reqwest::blocking::get("http://localhost:11434/api/tags")
        .map_err(|_| anyhow::anyhow!("not reachable"))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("unexpected status {}", resp.status())
    }
}

fn start_oll_background() -> Result<()> {
    if cfg!(target_os = "windows") {
        PCommand::new("cmd")
            .args(["/c", "start", "/b", "ollama", "serve"])
            .spawn()?;
    } else {
        PCommand::new("ollama")
            .args(["serve"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    Ok(())
}

// --- helpers ---

fn which(name: &str) -> bool {
    PCommand::new(if cfg!(target_os = "windows") { "where" } else { "which" })
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    PCommand::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn sysinfo_gb() -> f64 {
    if cfg!(target_os = "windows") {
        run_cmd("wmic", &["OS", "get", "TotalVisibleMemorySize", "/value"])
            .and_then(|s| {
                s.split('=')
                    .nth(1)
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .map(|kb| kb / 1_048_576.0)
            })
            .unwrap_or(0.0)
    } else if cfg!(target_os = "macos") {
        run_cmd("sysctl", &["hw.memsize"])
            .and_then(|s| s.split(':').nth(1).and_then(|v| v.trim().parse::<f64>().ok()))
            .map(|b| b / 1_073_741_824.0)
            .unwrap_or(0.0)
    } else {
        run_cmd("sh", &["-c", "grep MemTotal /proc/meminfo | awk '{print $2}'"])
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|kb| kb / 1_048_576.0)
            .unwrap_or(0.0)
    }
}

fn download_file(url: &str, dest: &std::path::Path) -> Result<()> {
    let resp = reqwest::blocking::get(url)
        .map_err(|e| anyhow::anyhow!("Download failed: {}", e))?;
    let bytes = resp.bytes()
        .map_err(|e| anyhow::anyhow!("Read response failed: {}", e))?;
    std::fs::write(dest, &bytes)?;
    Ok(())
}

pub fn format_system_info(info: &SystemInfo) -> String {
    let gpu_text = match &info.gpu {
        Some(g) => g.clone(),
        None => "No GPU detected (CPU-only)".into(),
    };
    format!(
        "System: {} ({})\nCPU: {} cores\nMemory: {:.0} GB\nGPU: {}\nOllama: {}",
        info.os, info.arch, info.cpu_cores, info.memory_gb, gpu_text,
        if info.has_ollama { "installed" } else { "not found" }
    )
}
