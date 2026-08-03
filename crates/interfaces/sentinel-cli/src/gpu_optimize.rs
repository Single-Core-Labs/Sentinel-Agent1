use async_trait::async_trait;
use sentinel_gpu_profiler::{bench, emulate, langs, optimizer, vram, GpuArch, GpuLanguage};
use sentinel_tools::{Tool, ToolContext, ToolOutput, ToolRegistry};
use std::sync::Arc;

/// Auto-Optimize Loop (design: docs/design/standout-roadmap.md §1).
///
/// Two shapes, one implementation:
/// - Standalone tool `gpu_optimize_kernel` (model-callable): analyze any kernel
///   file — sweep ~100 launch configs, report best config + bottleneck +
///   suggestions; opt-in real nvcc benchmark (`run_real_bench: true`) to "prove".
/// - Wrapper around the builtin `write`/`edit` tools (same tool name): after a
///   successful write/edit of a recognized GPU kernel file, appends a compact
///   `[gpu_optimize]` report to the tool result so the model sees measured
///   evidence on its next turn. Non-kernels pass through untouched.
pub struct GpuOptimizeKernelTool {
    inner: Option<Arc<dyn Tool>>,
}

impl GpuOptimizeKernelTool {
    /// Standalone, model-callable `gpu_optimize_kernel` tool.
    pub fn standalone() -> Self {
        Self { inner: None }
    }

    /// Wrapper mode: delegates to `inner` (the builtin `write`/`edit` tool)
    /// and appends the auto-optimization report to successful outputs.
    pub fn wrap(inner: Arc<dyn Tool>) -> Self {
        Self { inner: Some(inner) }
    }

    /// Replaces `write`/`edit` with auto-optimizing wrappers and registers the
    /// standalone `gpu_optimize_kernel` tool on `registry`. Call after
    /// `ToolRegistry::new()` and before the registry is handed to the agent.
    pub fn register_gpu_tools(registry: &mut ToolRegistry) {
        for name in ["write", "edit"] {
            if let Some(inner) = registry.get(name).cloned() {
                registry.register(Arc::new(Self::wrap(inner)));
            }
        }
        registry.register(Arc::new(Self::standalone()));
    }

    /// `langs::detect_language` never returns `Unknown` (it defaults to Cuda),
    /// so gate on extension/content explicitly: auto-optimize only real GPU
    /// kernel files, pass everything else through untouched.
    fn is_kernel_file(fname: &str, lang: GpuLanguage) -> bool {
        let lower = fname.to_lowercase();
        match lang {
            GpuLanguage::Cuda | GpuLanguage::CudaTile => {
                lower.ends_with(".cu") || lower.ends_with(".cuh") || lower.ends_with(".cuda")
            }
            GpuLanguage::Cute => {
                lower.ends_with(".cuh") || lower.ends_with(".hpp") || lower.ends_with(".h")
            }
            GpuLanguage::Mojo => lower.ends_with(".mojo") || lower.ends_with("🔥"),
            GpuLanguage::Triton
            | GpuLanguage::Numba
            | GpuLanguage::PyTorch
            | GpuLanguage::TileLang => lower.ends_with(".py"),
            GpuLanguage::Unknown => false,
        }
    }

    /// Map a compute-capability string ("8.6") to the sweep arch enum.
    fn arch_from_cc(cc: &str) -> Option<GpuArch> {
        match cc {
            "6.1" => Some(GpuArch::Pascal61),
            "7.0" => Some(GpuArch::Volta70),
            "7.5" => Some(GpuArch::Turing75),
            "8.0" => Some(GpuArch::Ampere80),
            "8.6" => Some(GpuArch::Ampere86),
            "8.9" => Some(GpuArch::Ada89),
            "9.0" => Some(GpuArch::Hopper90),
            "9.2" => Some(GpuArch::Hopper92),
            "10.0" => Some(GpuArch::Blackwell100),
            "10.2" => Some(GpuArch::Blackwell102),
            _ => None,
        }
    }

    /// Map a compute-capability or architecture string ("h100", "rtx4090", "b200", "8.9") to GpuArch.
    fn parse_arch_arg(raw: &str) -> Option<GpuArch> {
        let lower = raw.to_lowercase().replace(['_', '-'], "");
        if lower.contains("h100") || lower.contains("sm90") || lower == "9.0" {
            Some(GpuArch::Hopper90)
        } else if lower.contains("b200")
            || lower.contains("blackwell")
            || lower.contains("sm100")
            || lower == "10.0"
        {
            Some(GpuArch::Blackwell100)
        } else if lower.contains("4090")
            || lower.contains("ada")
            || lower.contains("sm89")
            || lower == "8.9"
        {
            Some(GpuArch::Ada89)
        } else if lower.contains("a100") || lower.contains("sm80") || lower == "8.0" {
            Some(GpuArch::Ampere80)
        } else if lower.contains("3090") || lower.contains("sm86") || lower == "8.6" {
            Some(GpuArch::Ampere86)
        } else if lower.contains("turing") || lower.contains("sm75") || lower == "7.5" {
            Some(GpuArch::Turing75)
        } else if lower.contains("volta") || lower.contains("sm70") || lower == "7.0" {
            Some(GpuArch::Volta70)
        } else if lower.contains("pascal") || lower.contains("sm61") || lower == "6.1" {
            Some(GpuArch::Pascal61)
        } else {
            Self::arch_from_cc(raw)
        }
    }

    /// Resolve the emulation target arch from user args or the real GPU; fall back to
    /// Ampere86 (SM86) when the machine reports nothing usable.
    fn resolve_arch_from_args(args: &serde_json::Value) -> GpuArch {
        if let Some(override_arch) = args["arch"].as_str() {
            if let Some(parsed) = Self::parse_arch_arg(override_arch) {
                return parsed;
            }
        }
        Self::resolve_arch()
    }

    fn resolve_arch() -> GpuArch {
        if let Some(name) = vram::detect_gpu_name() {
            if let Some(cc) = vram::compute_capability_from_name(&name) {
                if let Some(arch) = Self::arch_from_cc(&cc) {
                    return arch;
                }
            }
        }
        GpuArch::Ampere86
    }

    /// Compact, LLM-friendly report (≤ ~2.5k chars). The full sweep table is
    /// intentionally not dumped — the report only carries the evidence the
    /// model needs to rewrite the kernel.
    async fn build_report(
        fname: &str,
        source: &str,
        arch: GpuArch,
        run_real_bench: bool,
        gpu_name: Option<String>,
    ) -> Option<String> {
        let language = langs::detect_language(fname, source);
        if !Self::is_kernel_file(fname, language) {
            return None;
        }
        let arch_spec = emulate::arch_by_enum(arch);

        let configs = emulate::generate_sweep_configs(source);
        let entries = emulate::run_config_sweep(source, &configs, &arch);
        let best = emulate::detect_best_config(&entries)?;
        let bottleneck = optimizer::analyze_bottlenecks(&best.result);

        let gpu_line = match gpu_name {
            Some(name) => format!("{} ({})", arch_spec.name, name),
            None => arch_spec.name.to_string(),
        };

        let mut report = String::from("[gpu_optimize] ");
        report.push_str(fname);
        report.push_str(&format!(" — {} on {}", language.name(), gpu_line));
        report.push_str(&format!(
            "\nSwept {} configs. Best: {} — score {:.3} (est {:.2} us, occupancy {:.0}%, IPC {:.2})",
            entries.len(),
            best.label,
            best.score,
            best.result.execution_time_us,
            best.result.occupancy.occupancy_pct,
            best.result.ipc,
        ));

        if entries.len() > 1 {
            let runner_up = &entries[1];
            report.push_str(&format!(
                "\nRunner-up: {} — score {:.3}",
                runner_up.label, runner_up.score
            ));
        }

        report.push_str(&format!("\nBottleneck: {}", bottleneck.primary));
        for detail in bottleneck.details.iter().take(2) {
            report.push_str(&format!("\n  • {detail}"));
        }

        let hint = emulate::language_config_hint(language, arch_spec);
        if !hint.is_empty() {
            report.push_str(&format!("\nConfig hint: {}", hint.trim()));
        }

        report.push_str("\nSuggestions:");
        report.push_str("\n  1. Adopt the best config above (block/smem/registers).");
        for (i, detail) in bottleneck.details.iter().take(2).enumerate() {
            report.push_str(&format!("\n  {} {detail}", i + 2));
        }
        report.push_str("\n  3. Ask for `gpu_optimize_kernel` with run_real_bench=true to prove on real nvcc hardware.");

        if run_real_bench {
            let sm_count = vram::query_gpu_stats().sm_count.unwrap_or(80);
            let source = source.to_string();
            let fname = fname.to_string();
            report.push_str(&match tokio::task::spawn_blocking(move || {
                let result = bench::benchmark_kernel_real(&source, &fname, sm_count);
                bench::format_real_bench_result(&result)
            })
            .await
            {
                Ok(text) => format!("\n[real bench]\n{text}"),
                Err(e) => format!("\n[real bench] failed: {e}"),
            });
        }

        Some(report)
    }
}

impl GpuOptimizeKernelTool {
    fn file_path_arg(args: &serde_json::Value) -> Option<String> {
        args["file_path"]
            .as_str()
            .or_else(|| args["path"].as_str())
            .map(|s| s.to_string())
    }

    fn resolve_path(ctx: &ToolContext, raw: &str) -> String {
        let p = std::path::Path::new(raw);
        if p.is_absolute() {
            return raw.to_string();
        }
        if let Some(ref base) = ctx.sandbox_dir {
            let candidate = std::path::Path::new(base).join(raw);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
        if let Some(ref base) = ctx.workspace_dir {
            let candidate = std::path::Path::new(base).join(raw);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
        raw.to_string()
    }
}

#[async_trait]
impl Tool for GpuOptimizeKernelTool {
    fn name(&self) -> &str {
        match &self.inner {
            Some(inner) => inner.name(),
            None => "gpu_optimize_kernel",
        }
    }

    fn description(&self) -> &str {
        match &self.inner {
            Some(inner) => inner.description(),
            None => concat!(
                "Analyze a GPU kernel file (CUDA/Triton/Mojo/Numba/PyTorch/CUTE/TileLang), ",
                "sweep ~100 launch configurations on the detected GPU architecture, and report ",
                "the best config (block size, shared memory, registers), the primary bottleneck, ",
                "and concrete optimization suggestions. Args: ",
                "file_path (required), run_real_bench (bool, default false — compiles with nvcc ",
                "and times the kernel for real hardware proof), arch (optional override)."
            ),
        }
    }

    fn input_schema(&self) -> serde_json::Value {
        match &self.inner {
            Some(inner) => inner.input_schema(),
            None => serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the GPU kernel source file"
                    },
                    "run_real_bench": {
                        "type": "boolean",
                        "description": "Compile with nvcc and time the kernel for real hardware proof"
                    },
                    "arch": {
                        "type": "string",
                        "description": "Optional architecture override, e.g. sm_90 or H100"
                    }
                },
                "required": ["file_path"]
            }),
        }
    }

    fn is_mutating(&self) -> bool {
        match &self.inner {
            Some(inner) => inner.is_mutating(),
            None => false,
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let inner_output = match &self.inner {
            Some(inner) => Some(inner.execute(args.clone(), ctx).await),
            None => None,
        };
        if let Some(ref output) = inner_output {
            if output.is_error {
                return output.clone();
            }
        }

        let Some(raw_path) = Self::file_path_arg(&args) else {
            return match inner_output {
                Some(output) => output,
                None => ToolOutput::err("gpu_optimize_kernel: missing `file_path`"),
            };
        };
        let path = Self::resolve_path(ctx, &raw_path);
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                return match inner_output {
                    Some(output) => output,
                    None => ToolOutput::err(format!(
                        "gpu_optimize_kernel: cannot read '{}': {e}",
                        raw_path
                    )),
                };
            }
        };

        let fname = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&raw_path)
            .to_string();

        let run_real_bench = match &self.inner {
            Some(_) => false,
            None => args["run_real_bench"].as_bool().unwrap_or(false),
        };

        let gpu_name = vram::detect_gpu_name();
        let target_arch = Self::resolve_arch_from_args(&args);
        let report =
            Self::build_report(&fname, &source, target_arch, run_real_bench, gpu_name).await;

        match (&inner_output, report) {
            (Some(output), Some(report)) => {
                ToolOutput::ok(format!("{}\n\n{}", output.text.trim_end(), report))
            }
            (Some(output), None) => output.clone(),
            (None, Some(report)) => ToolOutput::ok(report),
            (None, None) => ToolOutput::err(format!(
                "gpu_optimize_kernel: '{}' is not a recognized GPU kernel source \
                 (supported: CUDA, Triton, Mojo, Numba, PyTorch, CUTE, CUDA Tile, TileLang)",
                fname
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KERNEL: &str = r#"__global__ void saxpy(float *y, const float *x, float a, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) y[i] = a * x[i] + y[i];
}"#;

    struct FakeWriteTool;
    #[async_trait]
    impl Tool for FakeWriteTool {
        fn name(&self) -> &str {
            "write"
        }
        fn description(&self) -> &str {
            "write a file"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
            let path = args["file_path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            if let Err(e) = std::fs::write(path, content) {
                return ToolOutput::err(format!("write failed: {e}"));
            }
            ToolOutput::ok(format!("Wrote {path}"))
        }
    }

    fn temp_kernel_path(name: &str, content: &str) -> String {
        let dir = std::env::temp_dir().join(format!("sentinel-gpu-opt-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn non_kernel_passthrough_does_not_append_report() {
        let ctx = ToolContext::new();
        let tool = GpuOptimizeKernelTool::wrap(Arc::new(FakeWriteTool));
        let p = temp_kernel_path("readme.md", "# docs\n");
        let out = tool
            .execute(
                serde_json::json!({ "file_path": p, "content": "# docs\n" }),
                &ctx,
            )
            .await;
        assert!(!out.is_error);
        assert!(!out.text.contains("[gpu_optimize]"));
    }

    #[tokio::test]
    async fn kernel_write_appends_optimization_report() {
        let ctx = ToolContext::new();
        let tool = GpuOptimizeKernelTool::wrap(Arc::new(FakeWriteTool));
        let p = temp_kernel_path("saxpy.cu", KERNEL);
        let out = tool
            .execute(
                serde_json::json!({ "file_path": p, "content": KERNEL }),
                &ctx,
            )
            .await;
        assert!(!out.is_error, "output: {}", out.text);
        assert!(out.text.contains("[gpu_optimize]"), "output: {}", out.text);
        assert!(out.text.contains("Best:"), "output: {}", out.text);
        assert!(out.text.contains("Bottleneck:"), "output: {}", out.text);
        assert!(out.text.contains("Suggestions:"), "output: {}", out.text);
        assert!(out.text.contains("Wrote"), "output: {}", out.text);
    }

    #[tokio::test]
    async fn standalone_tool_reports_kernel_analysis() {
        let ctx = ToolContext::new();
        let tool = GpuOptimizeKernelTool::standalone();
        let p = temp_kernel_path("vecadd.py", "import triton\n");
        let out = tool
            .execute(serde_json::json!({ "file_path": p }), &ctx)
            .await;
        assert!(!out.is_error, "output: {}", out.text);
        assert!(out.text.contains("[gpu_optimize]"));
        assert!(out.text.contains("Best:"));
        assert!(!out.text.contains("[real bench]"));
    }

    #[tokio::test]
    async fn standalone_non_kernel_returns_error() {
        let ctx = ToolContext::new();
        let tool = GpuOptimizeKernelTool::standalone();
        let p = temp_kernel_path("notes.txt", "plain text");
        let out = tool
            .execute(serde_json::json!({ "file_path": p }), &ctx)
            .await;
        assert!(out.is_error);
        assert!(out.text.contains("not a recognized GPU kernel"));
    }

    #[tokio::test]
    async fn missing_file_returns_error() {
        let ctx = ToolContext::new();
        let tool = GpuOptimizeKernelTool::standalone();
        let out = tool
            .execute(
                serde_json::json!({ "file_path": "C:\\definitely\\missing\\kern.cu" }),
                &ctx,
            )
            .await;
        assert!(out.is_error);
    }

    #[test]
    fn arch_resolution_falls_back_to_ampere86() {
        let arch = GpuOptimizeKernelTool::resolve_arch();
        let _ = arch; // resolves without panic on any machine
    }

    #[test]
    fn register_gpu_tools_replaces_write_and_adds_standalone() {
        let mut reg = ToolRegistry::new();
        GpuOptimizeKernelTool::register_gpu_tools(&mut reg);
        let names: Vec<String> = reg.list().iter().map(|d| d.name.clone()).collect();
        assert!(names.contains(&"gpu_optimize_kernel".to_string()));
        assert!(names.contains(&"write".to_string()));
        assert!(names.contains(&"edit".to_string()));
        let wrapper = reg.get("write").unwrap();
        assert!(wrapper
            .to_tool_def()
            .description
            .contains("Write content to a file"));
    }
}
