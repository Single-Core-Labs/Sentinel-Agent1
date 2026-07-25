# GPU Sandbox Architecture
## GPU Provisioning & Isolation for the Sentinel Agent

**Version:** 1.0
**Date:** 2026-07-25
**Status:** Design Draft

---

## Table of Contents

1. [Current State](#1-current-state)
2. [Target Architecture](#2-target-architecture)
3. [Sandbox Trait — Extended](#3-sandbox-trait--extended)
4. [DockerSandbox — Local GPU](#4-dockersandbox--local-gpu)
5. [CloudSandbox — Remote GPU](#5-cloudsandbox--remote-gpu)
6. [Sandbox Resolver — Auto-Selection](#6-sandbox-resolver--auto-selection)
7. [CLI Commands](#7-cli-commands)
8. [Agent Integration](#8-agent-integration)
9. [GPU Types & Pricing](#9-gpu-types--pricing)
10. [Cost Tracking & Budgeting](#10-cost-tracking--budgeting)
11. [Provider Integrations](#11-provider-integrations)
12. [Implementation Plan](#12-implementation-plan)

---

## 1. Current State

### 1.1 What Exists

```
crates/sentinel-core/src/sandbox.rs:

Sandbox trait ─┬─ NoSandbox  (passthrough, no isolation)
               └─ LocalSandbox (temp directory, filesystem-only)

Methods: exec(), read_file(), write_file(), destroy()
```

| Component | File | Status |
|-----------|------|--------|
| `Sandbox` trait | `crates/sentinel-core/src/sandbox.rs:6-13` | Implemented (6 methods) |
| `NoSandbox` | `crates/sentinel-core/src/sandbox.rs:15-38` | Implemented — passthrough |
| `LocalSandbox` | `crates/sentinel-core/src/sandbox.rs:40-103` | Implemented — temp dir isolation |
| `SharedSandbox` alias | `crates/sentinel-core/src/sandbox.rs:123` | Implemented |
| `DockerSandbox` | Not in code | Documented only in `docs/design/architecture.md` |
| Sandbox in CLI | `crates/sentinel-cli/src/exec.rs:91-94` | **Commented out** — disabled by default |
| `execSandboxed` endpoint | `crates/sentinel-app-server/src/handler.rs:379-381` | **Stubbed** — identical to regular exec |
| `CloudProvider` trait | `docs/paper-gpu-agent.md:176-184` | **Not implemented** — academic paper only |
| GPU detection | `crates/sentinel-ai-tui/src/local_model.rs:33-50` | Implemented — returns GPU name string |
| GPU CLI commands | None | **Not implemented** |

### 1.2 Key Gaps

1. No `DockerSandbox` — can't run GPU workloads in containers
2. No `CloudSandbox` — can't provision cloud GPU instances
3. Sandbox is commented out in CLI — users can't use it
4. `execSandboxed` is a stub — no actual isolation
5. No GPU type enum or pricing tables in code
6. `BashTool` ignores `Sandbox` — uses raw `tokio::process::Command`

---

## 2. Target Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        SANDBOX RESOLVER                             │
│                                                                     │
│  Input: { gpu_required: bool, vram_needed: u64, duration: u32 }     │
│                                                                     │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────────────┐  │
│  │ No GPU?  │───▶│ LocalSandbox │───▶│ Free, zero isolation      │  │
│  │          │    │ (CPU only)   │    │                           │  │
│  └──────────┘    └──────────────┘    └───────────────────────────┘  │
│                                                                     │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────────────┐  │
│  │ GPU      │───▶│ DockerSandbox│───▶│ Local NVIDIA GPU          │  │
│  │ <=48GB?  │    │              │    │ Docker container isolation│  │
│  └──────────┘    └──────────────┘    └───────────────────────────┘  │
│                                                                     │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────────────┐  │
│  │ GPU      │───▶│ CloudSandbox │───▶│ Cloud GPU instance        │  │
│  │ >48GB?   │    │              │    │ AWS/GCP/Lambda/RunPod     │  │
│  └──────────┘    └──────────────┘    └───────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
            │                              │
            ▼                              ▼
┌───────────────────────┐    ┌────────────────────────────┐
│   All sandboxes are   │    │  Agent tools (Bash, Read,  │
│   Sandbox trait impls │    │  Write) route through it   │
└───────────────────────┘    └────────────────────────────┘
```

### 2.1 Sandbox Hierarchy

```
                  ┌──────────────┐
                  │   Sandbox    │  (trait)
                  └──────┬──────┘
                         │
          ┌──────────────┼──────────────┐
          │              │              │
    ┌─────▼────┐  ┌─────▼────┐  ┌──────▼──────┐
    │NoSandbox │  │Local     │  │DockerSandbox│  (new)
    │(passthru)│  │Sandbox   │  └──────┬──────┘
    └──────────┘  └──────────┘         │
                                ┌──────▼──────┐
                                │CloudSandbox │  (new)
                                │             │
                                │  ┌────────┐ │
                                │  │ AWS    │ │
                                │  ├────────┤ │
                                │  │ GCP    │ │
                                │  ├────────┤ │
                                │  │Lambda  │ │
                                │  ├────────┤ │
                                │  │RunPod  │ │
                                │  └────────┘ │
                                └─────────────┘
```

---

## 3. Sandbox Trait — Extended

### 3.1 Current Trait (unchanged)

```rust
// crates/sentinel-core/src/sandbox.rs
#[async_trait]
pub trait Sandbox: Send + Sync {
    fn name(&self) -> &str;
    fn root(&self) -> &Path;
    async fn exec(&self, command: &str, workdir: &Path) -> Result<String, String>;
    async fn read_file(&self, path: &Path) -> Result<String, String>;
    async fn write_file(&self, path: &Path, content: &str) -> Result<(), String>;
    async fn destroy(&self);
}
```

### 3.2 New Additions

```rust
/// GPU specification for a sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSpec {
    pub gpu_type: GpuType,
    pub count: u32,           // number of GPUs (default: 1)
    pub vram_gb: u64,         // minimum VRAM per GPU
    pub cuda_version: Option<String>,
}

/// Supported GPU types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuType {
    // Local/Consumer
    #[serde(rename = "l4")]
    L4,              // NVIDIA L4        — 24GB  — $0.60/hr
    #[serde(rename = "l40s")]
    L40S,            // NVIDIA L40S      — 48GB  — $2.00/hr
    #[serde(rename = "rtx-4090")]
    Rtx4090,         // RTX 4090         — 24GB  — local only
    #[serde(rename = "rtx-6000")]
    Rtx6000Ada,      // RTX 6000 Ada     — 48GB  — local/cloud

    // Datacenter
    #[serde(rename = "a100-80gb")]
    A100_80GB,       // NVIDIA A100 80GB — 80GB  — $4.00/hr
    #[serde(rename = "h100")]
    H100,            // NVIDIA H100      — 80GB  — $12.00/hr
    #[serde(rename = "h200")]
    H200,            // NVIDIA H200      — 141GB — $8.00/hr
    #[serde(rename = "b200")]
    B200,            // NVIDIA B200      — 180GB — $16.00/hr

    // Multi-GPU
    #[serde(rename = "a100-80gb-x8")]
    A100_80GB_x8,    // 8× A100 80GB     — 640GB — $32.00/hr
    #[serde(rename = "h200-x8")]
    H200_x8,         // 8× H200          — 1.1TB — $64.00/hr

    // Apple Silicon
    #[serde(rename = "mps")]
    Mps,             // Apple Metal (MPS) — varies — local only
}

impl GpuType {
    pub fn vram_gb(&self) -> u64 {
        match self {
            GpuType::L4 | GpuType::Rtx4090 => 24,
            GpuType::L40S | GpuType::Rtx6000Ada => 48,
            GpuType::A100_80GB | GpuType::H100 => 80,
            GpuType::H200 => 141,
            GpuType::B200 => 180,
            GpuType::A100_80GB_x8 => 640,
            GpuType::H200_x8 => 1128,
            GpuType::Mps => 0, // depends on system
        }
    }

    pub fn price_per_hour(&self) -> f64 {
        match self {
            GpuType::L4 => 0.60,
            GpuType::L40S => 2.00,
            GpuType::Rtx4090 => 0.00,  // local
            GpuType::Rtx6000Ada => 0.00, // local
            GpuType::A100_80GB => 4.00,
            GpuType::H100 => 12.00,
            GpuType::H200 => 8.00,
            GpuType::B200 => 16.00,
            GpuType::A100_80GB_x8 => 32.00,
            GpuType::H200_x8 => 64.00,
            GpuType::Mps => 0.00,      // local
        }
    }

    pub fn is_cloud(&self) -> bool {
        matches!(self,
            GpuType::L4 | GpuType::L40S |
            GpuType::A100_80GB | GpuType::H100 |
            GpuType::H200 | GpuType::B200 |
            GpuType::A100_80GB_x8 | GpuType::H200_x8
        )
    }
}

/// Extended sandbox info (optional on all sandboxes)
#[derive(Debug, Clone, Serialize)]
pub struct SandboxInfo {
    pub name: String,
    pub kind: SandboxKind,
    pub gpu: Option<GpuType>,
    pub gpu_count: u32,
    pub vram_gb: u64,
    pub cost_per_hour: f64,
    pub started_at: DateTime<Utc>,
    pub ttl_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SandboxKind {
    NoSandbox,
    Local,
    Docker,
    Cloud { provider: String },
}
```

### 3.3 New Methods on Sandbox Trait (Optional)

```rust
#[async_trait]
pub trait Sandbox: Send + Sync {
    // ... existing methods ...

    /// Return sandbox metadata (optional, default returns None)
    fn info(&self) -> Option<SandboxInfo> { None }

    /// Check if sandbox is still alive
    async fn health_check(&self) -> Result<SandboxHealth, String> { Ok(SandboxHealth::Healthy) }

    /// Get estimated cost so far (returns USD)
    async fn accrued_cost(&self) -> f64 { 0.0 }

    /// Import a file into the sandbox (for uploads)
    async fn import_file(&self, local_path: &Path, sandbox_path: &Path) -> Result<(), String> {
        let content = tokio::fs::read_to_string(local_path).await.map_err(|e| e.to_string())?;
        self.write_file(sandbox_path, &content).await
    }

    /// Export a file from the sandbox (for downloads)
    async fn export_file(&self, sandbox_path: &Path, local_path: &Path) -> Result<(), String> {
        let content = self.read_file(sandbox_path).await?;
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
        }
        tokio::fs::write(local_path, &content).await.map_err(|e| e.to_string())
    }
}
```

---

## 4. DockerSandbox — Local GPU

### 4.1 Overview

Runs commands inside a Docker container with NVIDIA GPU passthrough. Uses the `bollard` Rust crate to communicate with the Docker daemon.

### 4.2 Implementation

```rust
// crates/sentinel-core/src/sandbox/docker.rs

use bollard::{Docker, container::*, image::*, exec::*};
use bollard::secret::*;

pub struct DockerSandbox {
    docker: Docker,
    container_id: String,
    image: String,
    root: PathBuf,           // workspace mount path
    gpu: Option<GpuType>,
    name: String,
    created_at: DateTime<Utc>,
    ttl_minutes: u32,
}

impl DockerSandbox {
    /// Create a new Docker sandbox.
    /// Pulls the image if not present, creates a container with GPU access,
    /// starts it, and keeps it running for exec commands.
    pub async fn create(
        config: DockerSandboxConfig,
    ) -> Result<Self, SandboxError> {

        let docker = Docker::connect_with_local_defaults()?;

        // 1. Ensure image exists
        docker.inspect_image(&config.image).await.or_else(|_| {
            docker.create_image(
                Some(CreateImageOptions { from_image: &config.image, ..Default::default() }),
                None,
                None,
            ).try_collect::<Vec<_>>().await
        })?;

        // 2. Create container
        let mut host_config = HostConfig::builder()
            .binds(vec![
                format!("{}:/workspace", config.workspace_dir.display()),
            ])
            .network_mode("bridge");

        // 3. Attach GPU if requested
        if let Some(gpu) = &config.gpu {
            let device_requests = vec![DeviceRequestBuilder::default()
                .capabilities(vec![vec!["gpu".to_string()]])
                .count(config.gpu_count as i64)
                .build()];
            host_config.device_requests(device_requests);
        }

        let create_opts = CreateContainerOptions {
            name: &config.container_name,
            ..Default::default()
        };

        let container = docker.create_container(
            Some(create_opts),
            Config {
                image: Some(&config.image),
                cmd: Some(vec!["sleep", "infinity"]),
                host_config: Some(host_config.build()),
                working_dir: Some("/workspace"),
                env: Some(config.env_vars.iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
                ..Default::default()
            },
        ).await?;

        // 4. Start container
        docker.start_container(&container.id, None).await?;

        Ok(Self {
            docker,
            container_id: container.id,
            image: config.image,
            root: config.workspace_dir,
            gpu: config.gpu,
            name: config.container_name,
            created_at: Utc::now(),
            ttl_minutes: config.ttl_minutes,
        })
    }
}

#[async_trait]
impl Sandbox for DockerSandbox {
    fn name(&self) -> &str { &self.name }

    fn root(&self) -> &Path { &self.root }

    async fn exec(&self, command: &str, workdir: &Path) -> Result<String, String> {
        let exec = self.docker.create_exec(
            &self.container_id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(vec!["sh", "-c", command]),
                working_dir: Some(workdir.to_string_lossy().to_string()),
                ..Default::default()
            },
        ).await.map_err(|e| e.to_string())?;

        let output = self.docker.start_exec(&exec.id, None)
            .await.map_err(|e| e.to_string())?
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.exit_code == Some(0) {
            Ok(stdout)
        } else {
            Err(if stderr.is_empty() { stdout } else { format!("{}\n{}", stdout, stderr) })
        }
    }

    async fn read_file(&self, path: &Path) -> Result<String, String> {
        // Use docker cp to copy file out, or exec cat
        self.exec(&format!("cat {}", path.display()), Path::new("/workspace")).await
    }

    async fn write_file(&self, path: &Path, content: &str) -> Result<(), String> {
        // Escape content and use tee/cat
        let escaped = content.replace('\'', "'\\''");
        self.exec(
            &format!("cat > {} << 'SANDBOX_EOF'\n{}\nSANDBOX_EOF", path.display(), content),
            Path::new("/workspace"),
        ).await?;
        Ok(())
    }

    async fn destroy(&self) {
        let _ = self.docker.stop_container(&self.container_id, None).await;
        let _ = self.docker.remove_container(
            &self.container_id,
            Some(RemoveContainerOptions { force: true, v: true }),
        ).await;
    }

    fn info(&self) -> Option<SandboxInfo> {
        Some(SandboxInfo {
            name: self.name.clone(),
            kind: SandboxKind::Docker,
            gpu: self.gpu,
            gpu_count: 1,
            vram_gb: self.gpu.map(|g| g.vram_gb()).unwrap_or(0),
            cost_per_hour: self.gpu.map(|g| g.price_per_hour()).unwrap_or(0.0),
            started_at: self.created_at,
            ttl_minutes: self.ttl_minutes,
        })
    }
}
```

### 4.3 Default Images

| Tag | Contents | Size |
|-----|----------|------|
| `sentinel/cuda:12.4-py311` | CUDA 12.4 + Python 3.11 + pip | 3.2 GB |
| `sentinel/cuda:12.4-torch` | Above + PyTorch 2.4 + transformers + peft | 6.8 GB |
| `sentinel/cuda:12.4-jax` | Above + JAX 0.4 + Flax | 5.1 GB |
| `sentinel/cpu:py311` | Python 3.11 only (no GPU) | 800 MB |

---

## 5. CloudSandbox — Remote GPU

### 5.1 Overview

Provisions a cloud GPU instance, installs dependencies, and exposes the same `Sandbox` trait interface. Commands execute over SSH. The instance auto-terminates after TTL.

### 5.2 CloudProvider Trait

```rust
// crates/sentinel-core/src/sandbox/cloud.rs

/// Unified interface for cloud GPU providers
#[async_trait]
pub trait CloudProvider: Send + Sync {
    /// Provider name ("aws", "gcp", "lambda", "runpod")
    fn name(&self) -> &str;

    /// List available GPU offerings with current pricing
    async fn list_gpus(&self) -> Result<Vec<GpuOffer>>;

    /// Provision a GPU instance
    async fn provision(&self, spec: &ProvisionSpec) -> Result<InstanceHandle>;

    /// Execute a command on the instance (SSH)
    async fn execute(&self, instance: &InstanceHandle, cmd: &str) -> Result<String>;

    /// Upload a file to the instance
    async fn upload(&self, instance: &InstanceHandle, local: &Path, remote: &Path) -> Result<()>;

    /// Download a file from the instance
    async fn download(&self, instance: &InstanceHandle, remote: &Path, local: &Path) -> Result<()>;

    /// Terminate the instance
    async fn teardown(&self, instance: &InstanceHandle) -> Result<()>;
}
```

### 5.3 Data Types

```rust
#[derive(Debug, Clone, Serialize)]
pub struct GpuOffer {
    pub provider: String,
    pub gpu_type: GpuType,
    pub count: u32,
    pub vram_gb: u64,
    pub price_per_hour: f64,
    pub spot_available: bool,
    /// Estimated time to ready (seconds)
    pub est_startup_seconds: u32,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionSpec {
    pub gpu_type: GpuType,
    pub gpu_count: u32,
    pub vram_gb: u64,
    pub duration_hours: u32,
    pub spot: bool,
    pub region: Option<String>,
    pub image: String,       // AMI or container image
    pub ssh_key: String,     // Public key to inject
}

#[derive(Debug, Clone)]
pub struct InstanceHandle {
    pub instance_id: String,
    pub provider: String,
    pub ip: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    pub gpu_type: GpuType,
    pub gpu_count: u32,
    pub region: String,
    pub price_per_hour: f64,
    pub launched_at: DateTime<Utc>,
    pub ttl: DateTime<Utc>,
}
```

### 5.4 CloudSandbox Implementation

```rust
pub struct CloudSandbox {
    provider: Box<dyn CloudProvider>,
    instance: InstanceHandle,
    ssh_key_path: PathBuf,
    workspace_dir: PathBuf,
    gpu: GpuType,
    name: String,
}

impl CloudSandbox {
    /// Provision a cloud GPU and return a Sandbox interface
    pub async fn provision(
        provider: Box<dyn CloudProvider>,
        spec: ProvisionSpec,
        workspace: &Path,
    ) -> Result<Self, SandboxError> {
        // 1. Provision instance
        let instance = provider.provision(&spec).await?;

        // 2. Wait for SSH readiness (poll up to 5 min)
        wait_for_ssh(&instance.ip, instance.ssh_port, Duration::from_secs(300)).await?;

        // 3. Install Docker + CUDA toolkit if not present
        let setup_commands = vec![
            "apt-get update -qq && apt-get install -y -qq docker.io nvidia-container-toolkit",
            "systemctl start docker",
            "docker pull sentinel/cuda:12.4-torch",
            "mkdir -p /workspace",
        ];
        for cmd in setup_commands {
            provider.execute(&instance, cmd).await?;
        }

        // 4. Upload workspace
        provider.upload(&instance, workspace, Path::new("/workspace")).await?;

        Ok(Self {
            provider,
            instance,
            ssh_key_path: PathBuf::from(&spec.ssh_key),
            workspace_dir: workspace.to_path_buf(),
            gpu: spec.gpu_type,
            name: format!("cloud-{}-{}", instance.provider, &instance.instance_id[..8]),
        })
    }
}

#[async_trait]
impl Sandbox for CloudSandbox {
    fn name(&self) -> &str { &self.name }
    fn root(&self) -> &Path { &self.workspace_dir }

    async fn exec(&self, command: &str, _workdir: &Path) -> Result<String, String> {
        self.provider.execute(&self.instance, command).await
            .map_err(|e| e.to_string())
    }

    async fn read_file(&self, path: &Path) -> Result<String, String> {
        self.provider.download(&self.instance, path, &self.workspace_dir.join("_tmp_read")).await
            .map_err(|e| e.to_string())?;
        tokio::fs::read_to_string(self.workspace_dir.join("_tmp_read")).await
            .map_err(|e| e.to_string())
    }

    async fn write_file(&self, path: &Path, content: &str) -> Result<(), String> {
        let tmp = self.workspace_dir.join("_tmp_write");
        tokio::fs::write(&tmp, content).await.map_err(|e| e.to_string())?;
        self.provider.upload(&self.instance, &tmp, path).await
            .map_err(|e| e.to_string())?;
        let _ = tokio::fs::remove_file(&tmp).await;
        Ok(())
    }

    async fn destroy(&self) {
        self.provider.teardown(&self.instance).await.ok();
    }

    fn info(&self) -> Option<SandboxInfo> {
        Some(SandboxInfo {
            name: self.name.clone(),
            kind: SandboxKind::Cloud { provider: self.provider.name().to_string() },
            gpu: Some(self.gpu),
            gpu_count: self.instance.gpu_count,
            vram_gb: self.gpu.vram_gb(),
            cost_per_hour: self.instance.price_per_hour,
            started_at: self.instance.launched_at,
            ttl_minutes: self.instance.ttl.signed_duration_since(self.instance.launched_at)
                .num_minutes() as u32,
        })
    }

    async fn accrued_cost(&self) -> f64 {
        let elapsed = Utc::now().signed_duration_since(self.instance.launched_at);
        let hours = elapsed.num_seconds() as f64 / 3600.0;
        hours * self.instance.price_per_hour
    }
}

/// Wait for SSH to become available
async fn wait_for_ssh(ip: &str, port: u16, timeout: Duration) -> Result<(), SandboxError> {
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(SandboxError::Timeout("SSH not ready".into()));
        }
        match tokio::net::TcpStream::connect(format!("{}:{}", ip, port)).await {
            Ok(_) => return Ok(()),
            Err(_) => tokio::time::sleep(Duration::from_secs(5)).await,
        }
    }
}
```

### 5.5 Provider Implementations

#### Lambda Labs

```rust
// crates/sentinel-core/src/sandbox/providers/lambda.rs

pub struct LambdaLabsProvider {
    api_key: String,
}

#[async_trait]
impl CloudProvider for LambdaLabsProvider {
    async fn list_gpus(&self) -> Result<Vec<GpuOffer>> {
        // GET https://api.lambdalabs.com/v1/instances
        // Returns available GPU types + pricing + regions
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<InstanceHandle> {
        // POST https://api.lambdalabs.com/v1/instance-operations/launch
        // {
        //   "region_name": "us-west-1",
        //   "instance_type_name": "gpu_1x_h200",
        //   "ssh_key_names": ["my-key"]
        // }
    }

    async fn execute(&self, instance: &InstanceHandle, cmd: &str) -> Result<String> {
        // ssh -i key.pem ubuntu@{ip} "{cmd}"
        // Returns stdout
    }

    async fn teardown(&self, instance: &InstanceHandle) -> Result<()> {
        // POST https://api.lambdalabs.com/v1/instance-operations/terminate
    }
}
```

#### RunPod

```rust
pub struct RunPodProvider {
    api_key: String,
}

#[async_trait]
impl CloudProvider for RunPodProvider {
    // Uses RunPod Serverless or Pods API
    // Simpler than Lambda: pods come pre-configured with Docker
    // execute() → runpodctl send + runpodctl receive
}
```

#### AWS

```rust
pub struct AwsProvider {
    access_key: String,
    secret_key: String,
    region: String,
}

#[async_trait]
impl CloudProvider for AwsProvider {
    // Uses aws-sdk-ec2 crate
    // provision() → EC2 RunInstances with G6e/G5 instance types
    // execute() → SSM RunCommand or SSH
    // teardown() → EC2 TerminateInstances
}
```

---

## 6. Sandbox Resolver — Auto-Selection

### 6.1 Logic

```rust
// crates/sentinel-core/src/sandbox/resolver.rs

pub struct SandboxResolver {
    local_detector: LocalGpuDetector,
    cloud_providers: Vec<Box<dyn CloudProvider>>,
    config: SandboxConfig,
}

impl SandboxResolver {
    /// Given a task requirement, select and create the best sandbox
    pub async fn resolve(&self, requirement: &TaskRequirement) -> Result<Box<dyn Sandbox>, SandboxError> {
        // 1. Check local GPU availability
        let local_gpu = self.local_detector.detect().await;

        // 2. If task needs no GPU, use LocalSandbox
        if !requirement.needs_gpu {
            return Ok(Box::new(LocalSandbox::new(&self.config.workspace)?));
        }

        // 3. If local GPU has enough VRAM, use DockerSandbox
        if let Some(gpu) = &local_gpu {
            if gpu.vram_gb >= requirement.vram_gb {
                return Ok(Box::new(
                    DockerSandbox::create(DockerSandboxConfig {
                        gpu: Some(gpu.gpu_type),
                        gpu_count: 1.min(gpu.count),
                        workspace_dir: self.config.workspace.clone(),
                        image: select_image(requirement),
                        ..Default::default()
                    }).await?
                ));
            }
        }

        // 4. Local GPU insufficient → check cloud providers
        let mut best_offer: Option<(f64, GpuOffer, Box<dyn CloudProvider>)> = None;

        for provider in &self.cloud_providers {
            let offers = provider.list_gpus().await?;
            for offer in offers {
                if offer.gpu_type.vram_gb() >= requirement.vram_gb {
                    // Score: lower is better
                    let score = offer.price_per_hour
                        + (offer.est_startup_seconds as f64 / 60.0) * 0.01; // $0.01/min wait penalty
                    match &best_offer {
                        Some((best_score, _, _)) if score < *best_score => {
                            best_offer = Some((score, offer, provider.provider_clone()));
                        }
                        None => {
                            best_offer = Some((score, offer, provider.provider_clone()));
                        }
                        _ => {}
                    }
                }
            }
        }

        // 5. Provision the cheapest suitable cloud GPU
        if let Some((_, offer, provider)) = best_offer {
            let spec = ProvisionSpec {
                gpu_type: offer.gpu_type,
                gpu_count: offer.count,
                vram_gb: offer.vram_gb,
                duration_hours: requirement.est_hours.max(1),
                spot: true,
                region: Some(offer.region.clone()),
                image: select_image(requirement),
                ssh_key: self.config.ssh_public_key.clone(),
            };
            return Ok(Box::new(
                CloudSandbox::provision(provider, spec, &self.config.workspace).await?
            ));
        }

        Err(SandboxError::NoGpuAvailable {
            vram_required: requirement.vram_gb,
            local_gpu: local_gpu.map(|g| format!("{:?} ({}GB)", g.gpu_type, g.vram_gb)),
        })
    }
}
```

### 6.2 Decision Flow

```
Task: "Fine-tune Llama 3.1 70B with LoRA"
  → requires ~80GB VRAM, ~2 hours

SandboxResolver::resolve()
  │
  ├─ detect_local_gpu()
  │   └─ RTX 4090 (24GB) — insufficient
  │
  ├─ list_cloud_gpus()
  │   ├─ Lambda: H200 141GB $8/hr, A100 80GB $4/hr
  │   ├─ RunPod: H100 80GB $3.89/hr, A100 80GB $3.49/hr
  │   └─ AWS: G6e.12xlarge (H100) $6.12/hr spot
  │
  ├─ score_candidates()
  │   └─ Winner: RunPod A100-80GB @ $3.49/hr (cheapest + available)
  │
  ├─ provision_cloud(RunPod, A100-80GB, 2h)
  │   └─ Instance ready in 47s
  │
  └─ return CloudSandbox { ... }
```

---

## 7. CLI Commands

### 7.1 New Subcommand

```rust
// crates/sentinel-cli/src/main.rs

#[derive(clap::Subcommand)]
enum Command {
    /// Provision and manage GPU sandboxes
    Sandbox {
        #[clap(subcommand)]
        action: SandboxAction,
    },
    // ... existing commands (exec, ai, auth, server, proxy, tui, diagnostics)
}

#[derive(clap::Subcommand)]
enum SandboxAction {
    /// Create a new sandbox
    Create {
        /// GPU type (h200, a100-80gb, l40s, etc.)
        #[clap(long)]
        gpu: Option<String>,

        /// Number of GPUs
        #[clap(long, default_value = "1")]
        count: u32,

        /// Duration in hours (instance auto-terminates after this)
        #[clap(long, default_value = "2")]
        hours: u32,

        /// Use spot pricing (default: true)
        #[clap(long)]
        spot: Option<bool>,
    },

    /// List active sandboxes
    List,

    /// Show sandbox info
    Status {
        id: String,
    },

    /// Connect to sandbox via SSH
    Connect {
        id: String,
    },

    /// Run a command in the sandbox
    Exec {
        id: String,
        command: String,
    },

    /// Upload a file to the sandbox
    Upload {
        id: String,
        local: String,
        remote: String,
    },

    /// Download a file from the sandbox
    Download {
        id: String,
        remote: String,
        local: String,
    },

    /// Destroy a sandbox (terminates instance)
    Destroy {
        id: String,
    },

    /// Estimate cost without provisioning
    Estimate {
        /// GPU type
        gpu: String,
        /// Duration in hours
        #[clap(long, default_value = "1")]
        hours: u32,
        /// Use spot pricing
        #[clap(long)]
        spot: Option<bool>,
    },
}
```

### 7.2 User Experience

```bash
# List available GPU types with pricing
$ sentinel sandbox estimate --gpu h200 --hours 2
  GPU     | VRAM  | Provider     | Price/hr | Est. Total
  ────────┼───────┼──────────────┼──────────┼───────────
  H200    | 141GB | Lambda Labs  | $8.00    | $16.00
  H200    | 141GB | RunPod       | $7.99    | $15.98
  A100-80 | 80GB  | RunPod       | $3.49    | $6.98
  A100-80 | 80GB  | Lambda Labs  | $4.00    | $8.00

# Create a sandbox (auto-selects cheapest option)
$ sentinel sandbox create --gpu h200 --hours 2
  ✓ Provisioning H200 on RunPod (us-west-1)...
  ✓ Instance ready in 47s
  ✓ Sandbox "cloud-runpod-a1b2c3d4" active
  ✓ Cost: $6.98 — Auto-terminates at 2026-07-25 14:00:00 UTC
  $ sentinel sandbox connect cloud-runpod-a1b2c3d4
  # or run commands:
  $ sentinel sandbox exec cloud-runpod-a1b2c3d4 -- nvidia-smi

# List active sandboxes
$ sentinel sandbox list
  ID                    | GPU   | Provider | Runtime | Cost   | TTL
  ──────────────────────┼───────┼──────────┼─────────┼────────┼─────
  cloud-runpod-a1b2c3d4 | H200  | RunPod   | 23m     | $2.66  | 1h37m
  docker-sandbox-...    | L4    | Local    | 5m      | $0.05  | 55m

# Destroy when done
$ sentinel sandbox destroy cloud-runpod-a1b2c3d4
  ✓ Terminating RunPod instance a1b2c3d4
  ✓ Total cost: $3.49
  ✓ Saved checkpoint to ./checkpoints/
```

---

## 8. Agent Integration

### 8.1 Sandbox-Aware Tool Execution

```rust
// crates/sentinel-core/src/agent.rs

pub struct Agent {
    // ...
    sandbox_manager: SandboxManager,
}

impl Agent {
    /// Execute tools with sandbox routing
    async fn execute_tools_sandboxed(
        &self,
        tool_calls: Vec<ToolCall>,
        thread: &AgentThread,
    ) -> Result<Vec<ToolResult>> {
        let sandbox = self.sandbox_manager.current().await;

        for call in &tool_calls {
            match call.tool_name {
                "bash" => {
                    let cmd = call.args["command"].as_str().unwrap();
                    let output = sandbox.exec(cmd, sandbox.root()).await?;
                    // ...
                }
                "read" => {
                    let path = call.args["file_path"].as_str().unwrap();
                    let content = sandbox.read_file(Path::new(path)).await?;
                    // ...
                }
                "write" | "edit" => {
                    let path = call.args["file_path"].as_str().unwrap();
                    let content = call.args["content"].as_str().unwrap();
                    sandbox.write_file(Path::new(path), content).await?;
                    // ...
                }
                "upload" => {
                    let local = call.args["local"].as_str().unwrap();
                    let remote = call.args["remote"].as_str().unwrap();
                    sandbox.import_file(Path::new(local), Path::new(remote)).await?;
                }
                "download" => {
                    let remote = call.args["remote"].as_str().unwrap();
                    let local = call.args["local"].as_str().unwrap();
                    sandbox.export_file(Path::new(remote), Path::new(local)).await?;
                }
                _ => {}
            }
        }

        // ...
    }
}
```

### 8.2 New Agent Tools

```rust
// crates/sentinel-tools/src/builtin.rs

// Add these tools to the agent's toolbox:

Tool::new("sandbox_create")
    .description("Provision a GPU sandbox for compute-heavy tasks")
    .parameters(json!({
        "type": "object",
        "properties": {
            "gpu": { "type": "string", "description": "GPU type: h200, a100-80gb, l40s" },
            "hours": { "type": "number", "description": "Duration in hours" },
            "task": { "type": "string", "description": "What you'll run (for image selection)" },
        }
    }))
    .handler(|args, ctx| async {
        let gpu = args["gpu"].as_str().unwrap_or("auto");
        let hours = args["hours"].as_f64().unwrap_or(1.0);
        let task = args["task"].as_str().unwrap_or("ml");

        // Show cost estimate
        let estimate = ctx.sandbox_manager.estimate(gpu, hours).await;
        ctx.send_event(AgentEvent::CostEstimate { gpu, hours, cost: estimate });

        // Ask approval
        ctx.request_approval(format!("Provision {} GPU for {:.1}h (${:.2})?", gpu, hours, estimate));

        // Provision
        let sandbox = ctx.sandbox_manager.create(gpu, hours).await?;
        Ok(json!({ "sandbox_id": sandbox.name(), "cost_per_hour": estimate / hours }))
    });

Tool::new("sandbox_exec")
    .description("Run a command inside the active sandbox")
    .parameters(json!({
        "type": "object",
        "properties": {
            "command": { "type": "string" },
            "workdir": { "type": "string", "default": "/workspace" },
        }
    }))
    .handler(|args, ctx| async {
        let cmd = args["command"].as_str().unwrap();
        let wd = args["workdir"].as_str().unwrap_or("/workspace");
        let output = ctx.sandbox_manager.current().await?.exec(cmd, Path::new(wd)).await?;
        Ok(json!({ "output": output }))
    });

Tool::new("sandbox_destroy")
    .description("Destroy the active sandbox and download results")
    .parameters(json!({
        "type": "object",
        "properties": {
            "save_paths": { "type": "array", "items": { "type": "string" } },
        }
    }));

Tool::new("sandbox_status")
    .description("Check sandbox health, cost accrued, and time remaining");
```

### 8.3 Agent Workflow

```
User: "Fine-tune Llama 3.1 70B on our dataset"

Agent:
  Step 1: Analyze requirements
    → Model needs ~80GB VRAM
    → Local GPU (RTX 4090, 24GB) insufficient
    → Needs cloud GPU

  Step 2: Query cloud providers
    → RunPod A100-80GB: $3.49/hr available
    → Lambda H200: $8.00/hr available

  Step 3: Show estimate + ask approval
    Agent: "This task needs ~80GB VRAM.
            Recommended: A100-80GB on RunPod.
            Estimated cost: $6.98 for 2 hours.
            Provision? (y/N)"

  Step 4: User approves → provision
    ✓ Instance ready in 47s
    ✓ Workspace uploaded
    ✓ Dependencies installed

  Step 5: Execute training in sandbox
    Agent runs:
      sandbox_exec("git clone https://.../train.git")
      sandbox_exec("python train.py --model llama-3.1-70b --lora --lr 1e-4")
      sandbox_exec("tail -f logs/training.log")  (streaming output)

  Step 6: Monitor + recover from failures
    → OOM detected: reduce batch size, retry
    → Training complete
    → Download checkpoints

  Step 7: Destroy sandbox
    → Total cost: $6.98
    → Checkpoint saved to ./outputs/lora-llama-70b/
```

---

## 9. GPU Types & Pricing

### 9.1 Standard Pricing Table

| GPU | VRAM | Type | Cloud $/hr | Spot $/hr | Local |
|-----|------|------|-----------|-----------|-------|
| L4 | 24GB | Entry | $0.60 | $0.18 | No |
| RTX 4090 | 24GB | Consumer | — | — | Yes |
| L40S | 48GB | Mid | $2.00 | $0.60 | No |
| RTX 6000 Ada | 48GB | Workstation | — | — | Yes |
| A100-80GB | 80GB | High-end | $4.00 | $1.20 | No |
| H100 | 80GB | High-end | $12.00 | $3.60 | No |
| H200 | 141GB | Flagship | $8.00 | $2.40 | No |
| B200 | 180GB | Flagship | $16.00 | $4.80 | No |
| 8× A100-80GB | 640GB | Multi-GPU | $32.00 | $9.60 | No |
| 8× H200 | 1.1TB | Multi-GPU | $64.00 | $19.20 | No |

### 9.2 Model → GPU Mapping

| Model Size | Fine-tuning (LoRA) | Fine-tuning (Full) | Inference |
|-----------|-------------------|-------------------|-----------|
| 1-3B | CPU / L4 (24GB) | L4 (24GB) | CPU / L4 |
| 7-8B | L4 (24GB) | L40S (48GB) | L4 (24GB) |
| 13-20B | L40S (48GB) | A100-80GB | L40S (48GB) |
| 34-40B | A100-80GB | H200 (141GB) | A100-80GB |
| 70B | A100-80GB | H200 (141GB) | H200 (141GB) |
| 120B+ | H200 (141GB) | 8× H200 | H200 (141GB) |

### 9.3 Cost Estimation Formula

```
total_cost = (provision_time + runtime) × price_per_hour
           + storage_cost × checkpoint_size_gb × months_stored
           + data_transfer_gb × transfer_price

provision_time:
  - DockerSandbox: 30-90s (image pull + container start)
  - CloudSandbox (cold): 30-180s (instance boot + SSH + setup)
  - CloudSandbox (warm pool): 5-10s

data_transfer:
  - Upload dataset: free (to cloud region)
  - Download checkpoint: $0.01-0.09/GB (egress)
```

---

## 10. Cost Tracking & Budgeting

### 10.1 BudgetGuard Integration

```rust
// The existing BudgetGuard from sentinel-core is extended for GPU

pub struct SandboxBudgetGuard {
    daily_cap: f64,           // $50/day default
    monthly_cap: f64,         // $500/month default
    spent_today: Arc<AtomicF64>,
    active_sandboxes: Arc<RwLock<Vec<SandboxBudgetEntry>>>,
}

struct SandboxBudgetEntry {
    sandbox_id: String,
    gpu_type: GpuType,
    hourly_rate: f64,
    started_at: DateTime<Utc>,
    max_hours: u32,
}

impl SandboxBudgetGuard {
    /// Check if provisioning is within budget
    async fn try_reserve(&self, spec: &ProvisionSpec) -> Result<(), BudgetError> {
        let estimated_cost = spec.gpu_type.price_per_hour() * spec.gpu_count as f64
            * spec.duration_hours as f64;
        let current_spend = self.spent_today.load(Ordering::Relaxed);

        if current_spend + estimated_cost > self.daily_cap {
            return Err(BudgetError::DailyCapExceeded {
                cap: self.daily_cap,
                estimated: estimated_cost,
                current: current_spend,
            });
        }
        Ok(())
    }

    /// Track sandbox cost in real-time
    async fn track(&self, entry: SandboxBudgetEntry) {
        let mut active = self.active_sandboxes.write().await;
        active.push(entry);
        // Spawn background task to update spent_today every 60s
    }
}
```

### 10.2 Usage Reporting

```json
// Per-sandbox report (logged to Postgres via sentinel-analytics)
{
  "sandbox_id": "cloud-runpod-a1b2c3d4",
  "user_id": "user_abc123",
  "session_id": "sess_xyz",
  "gpu_type": "A100-80GB",
  "gpu_count": 1,
  "provider": "runpod",
  "region": "us-west-1",
  "duration_seconds": 7200,
  "cost_usd": 6.98,
  "tasks_completed": ["download_dataset", "train_lora", "evaluate", "save_checkpoint"],
  "status": "completed",
  "created_at": "2026-07-25T12:00:00Z",
  "destroyed_at": "2026-07-25T14:00:00Z"
}
```

---

## 11. Provider Integrations

### 11.1 Provider Comparison

| Provider | API Type | GPU Types | Spot | Min Billing | Startup | Best For |
|----------|----------|-----------|------|-------------|---------|----------|
| **Lambda Labs** | REST | H200, A100, L40S | Yes | 1 sec | 30-60s | Simple API, fast startup |
| **RunPod** | REST | H100, A100, L40S, RTX | Serverless | 1 sec | 10-30s | Cheapest spot, fast cold start |
| **AWS (EC2)** | SDK (Rust) | All (G5, G6, P5) | Yes | 1 hour | 60-180s | Enterprise standard |
| **GCP (GKE)** | SDK (Rust) | L4, A100, H100 | Yes (preemptible) | 1 min | 30-120s | GKE integration |
| **Azure** | SDK | ND-series | Yes (low-priority) | 1 hour | 60-180s | Enterprise |
| **Vast.ai** | REST | All types | Yes (auction) | 1 hour | 60s-24h | Cheapest, variable quality |
| **Modal** | Python SDK | A100-80GB, H100 | Yes | 1 sec | 5-30s | Serverless, no infra mgmt |
| **CoreWeave** | REST | H100, A100, L40S | Yes | 1 sec | 10-60s | ML-optimized infra |

### 11.2 Recommended Provider Order

```
Priority 1: RunPod (cheapest, fastest cold start, serverless)
Priority 2: Lambda Labs (simple API, good availability)
Priority 3: AWS EC2 Spot (enterprise, unlimited scale)
Fallback:    GCP Preemptible (if others unavailable)
```

---

## 12. Implementation Plan

### Phase 1: DockerSandbox + CLI (2 weeks)

| Step | What | Files | Depends On |
|------|------|-------|-----------|
| 1.1 | Add `bollard` dependency to `sentinel-core` | `Cargo.toml` | — |
| 1.2 | Implement `DockerSandbox` | `crates/sentinel-core/src/sandbox/docker.rs` | 1.1 |
| 1.3 | Add `GpuType` + `GpuSpec` + `SandboxInfo` types | `crates/sentinel-core/src/sandbox/types.rs` | — |
| 1.4 | Wire `DockerSandbox` into `cli/src/exec.rs` (uncomment + use) | `crates/sentinel-cli/src/exec.rs` | 1.2 |
| 1.5 | Add `sentinel sandbox` subcommands (create, list, exec, destroy) | `crates/sentinel-cli/src/main.rs` | 1.2 |
| 1.6 | Wire all tools (bash, read, write) through sandbox in agent loop | `crates/sentinel-core/src/agent.rs` | 1.4 |
| 1.7 | Tests | `tests/` | 1.2-1.6 |

### Phase 2: CloudSandbox — Lambda Labs (3 weeks)

| Step | What | Files | Depends On |
|------|------|-------|-----------|
| 2.1 | Implement `CloudProvider` trait + data types | `crates/sentinel-core/src/sandbox/cloud.rs` | Phase 1 |
| 2.2 | Lambda Labs provider | `crates/sentinel-core/src/sandbox/providers/lambda.rs` | 2.1 |
| 2.3 | `CloudSandbox` provisioning + SSH exec | `crates/sentinel-core/src/sandbox/cloud.rs` | 2.1-2.2 |
| 2.4 | `SandboxResolver` (auto-select cheapest) | `crates/sentinel-core/src/sandbox/resolver.rs` | 2.3 |
| 2.5 | Wire resolver into agent (auto provision when GPU needed) | `crates/sentinel-core/src/agent.rs` | 2.4 |
| 2.6 | `sentinel sandbox connect` (SSH) + `sentinel sandbox status` | `crates/sentinel-cli/src/main.rs` | 2.3 |

### Phase 3: RunPod + AWS Providers (2 weeks)

| Step | What | Files | Depends On |
|------|------|-------|-----------|
| 3.1 | RunPod provider | `crates/sentinel-core/src/sandbox/providers/runpod.rs` | Phase 2 |
| 3.2 | AWS provider (EC2 + SSM) | `crates/sentinel-core/src/sandbox/providers/aws.rs` | Phase 2 |
| 3.3 | Multi-provider scoring + fallback | `crates/sentinel-core/src/sandbox/resolver.rs` | 3.1-3.2 |

### Phase 4: Budget + Cost Tracking + Admin (1 week)

| Step | What | Files | Depends On |
|------|------|-------|-----------|
| 4.1 | Wire `SandboxBudgetGuard` into agent | `crates/sentinel-core/src/budget.rs` | Phase 2 |
| 4.2 | Send usage events to `sentinel-analytics` | `crates/sentinel-analytics/` | 4.1 |
| 4.3 | Admin dashboard sandbox tab | `frontend/src/pages/SandboxDashboard.tsx` | 4.2 |
| 4.4 | `sentinel sandbox estimate` pricing display | `crates/sentinel-cli/` | Phase 2 |

### Phase 5: Agent Tools + Auto-Orchestration (2 weeks)

| Step | What | Files | Depends On |
|------|------|-------|-----------|
| 5.1 | `sandbox_create`, `sandbox_exec`, `sandbox_destroy`, `sandbox_status` tools | `crates/sentinel-tools/src/builtin.rs` | Phase 2 |
| 5.2 | Auto-detect GPU requirement from task description | `crates/sentinel-core/src/agent.rs` | 5.1 |
| 5.3 | Approval flow: estimate → ask → provision → run → destroy | Agent integration | 5.2 |
| 5.4 | Failure recovery: OOM → reduce batch size, preempt → switch provider | `crates/sentinel-core/src/agent.rs` | 5.3 |

### Phase 6: Warm Pool + Multi-GPU (2 weeks, optional)

| Step | What | Files | Depends On |
|------|------|-------|-----------|
| 6.1 | Warm instance pool (keep N instances ready) | `crates/sentinel-core/src/sandbox/pool.rs` | Phase 2 |
| 6.2 | Multi-GPU distributed training support | `crates/sentinel-core/src/sandbox/cloud.rs` | Phase 3 |
| 6.3 | Apple Metal (MPS) DockerSandbox for macOS | `crates/sentinel-core/src/sandbox/docker.rs` | Phase 1 |

---

## Appendix: File Map

### Existing Files (to modify)

```
crates/sentinel-core/src/sandbox.rs         — Add module declarations, keep existing types
crates/sentinel-core/src/lib.rs             — Add `pub mod sandbox` sub-modules
crates/sentinel-core/src/agent.rs           — Wire sandbox into tool execution
crates/sentinel-core/src/budget.rs          — Add GPU budget tracking
crates/sentinel-cli/src/main.rs             — Add `Sandbox` subcommand
crates/sentinel-cli/src/exec.rs             — Uncomment and wire sandbox
crates/sentinel-tools/src/builtin.rs        — Add sandbox_create/exec/destroy/status tools
crates/sentinel-tools/src/tool.rs           — Add sandbox fields to ToolContext
crates/sentinel-app-server/src/handler.rs   — Fix execSandboxed stub
crates/sentinel-core/Cargo.toml             — Add bollard, ssh2, rusoto dependencies
```

### New Files (to create)

```
crates/sentinel-core/src/sandbox/
├── mod.rs                 — Module declarations + re-exports
├── types.rs               — GpuType, GpuSpec, SandboxInfo, SandboxKind, TaskRequirement
├── docker.rs              — DockerSandbox implementation
├── cloud.rs               — CloudProvider trait + CloudSandbox implementation
├── resolver.rs            — SandboxResolver (auto-select best GPU)
├── pool.rs                — Warm instance pool (Phase 6)
├── providers/
│   ├── mod.rs             — Provider factory
│   ├── lambda.rs          — Lambda Labs provider
│   ├── runpod.rs          — RunPod provider
│   └── aws.rs             — AWS EC2 provider

frontend/src/pages/
├── SandboxDashboard.tsx   — Active sandboxes, create/destroy UI
├── SandboxCreateDialog.tsx— GPU selector, cost estimate, provision button
```

### Documentation

```
docs/GPU_SANDBOX_ARCHITECTURE.md  — This document
```

---

*This document describes the full GPU sandbox architecture. Implementation follows the phased plan: DockerSandbox first (local GPU in containers), then CloudSandbox (provisioned cloud GPU), finally auto-orchestration (agent auto-detects requirements and provisions).*
