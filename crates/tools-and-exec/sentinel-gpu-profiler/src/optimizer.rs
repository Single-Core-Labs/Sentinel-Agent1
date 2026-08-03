use crate::emulate::EmulationResult;
use crate::{emulate, GpuArch, GpuLanguage, LaunchConfig};

#[derive(Debug, Clone)]
pub struct BottleneckReport {
    pub primary: &'static str,
    pub secondary: Vec<String>,
    pub details: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SpeedupResult {
    pub before_cycles: u64,
    pub after_cycles: u64,
    pub before_time_us: f64,
    pub after_time_us: f64,
    pub speedup_x: f64,
    pub improvement_pct: f64,
}

#[derive(Debug, Clone)]
pub struct OptimizeRequest {
    pub source: String,
    pub filename: String,
    pub language: GpuLanguage,
    pub target_arch: GpuArch,
    pub launch_config: LaunchConfig,
    pub ncu_profile: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OptimizeOutput {
    pub original_source: String,
    pub optimized_source: String,
    pub diff: String,
    pub bottleneck_report: BottleneckReport,
    pub speedup_estimate: Option<SpeedupResult>,
    pub llm_optimization_notes: String,
    pub compiled_ok: Option<bool>,
    pub correctness_passed: Option<bool>,
}

pub fn analyze_bottlenecks(result: &EmulationResult) -> BottleneckReport {
    let mut details = Vec::new();
    let mut secondary = Vec::new();

    if result.memory.coalescing_efficiency < 0.5 {
        details.push(format!(
            "Poor coalescing ({:.0}%) — strided global memory access",
            result.memory.coalescing_efficiency * 100.0
        ));
    }
    if result.memory.shared_bank_conflicts > 0 {
        details.push(format!(
            "{} shared memory bank conflicts",
            result.memory.shared_bank_conflicts
        ));
    }
    if result.memory.register_spills > 0 {
        details.push(format!(
            "{} register spills — consider __launch_bounds__",
            result.memory.register_spills
        ));
        secondary.push("Register pressure".into());
    }
    if result.occupancy.occupancy_pct < 50.0 {
        details.push(format!(
            "Low occupancy ({:.0}%) — blocked by {}",
            result.occupancy.occupancy_pct, result.occupancy.limiting_factor
        ));
        secondary.push("Occupancy-limited".into());
    }
    if result.sm_util_pct < 50.0 {
        details.push(format!(
            "Low SM utilization ({:.0}%) — warps stalled on memory",
            result.sm_util_pct
        ));
        secondary.push("Low SM utilization".into());
    }

    let primary = match result.bottleneck {
        "Compute-bound" => {
            if result.occupancy.occupancy_pct < 60.0 {
                details.push("Compute-bound with low occupancy — increasing occupancy may improve throughput".into());
                "Compute-bound (occupancy-limited)"
            } else {
                "Compute-bound"
            }
        }
        "Memory-bound" => {
            if result.memory.coalescing_efficiency < 0.75 {
                "Memory-bound (poor coalescing)"
            } else {
                "Memory-bound"
            }
        }
        _ => result.bottleneck,
    };

    if details.is_empty() {
        details.push("No major bottlenecks detected".into());
    }

    BottleneckReport {
        primary,
        secondary,
        details,
    }
}

pub fn estimate_speedup(before: &EmulationResult, after: &EmulationResult) -> SpeedupResult {
    let before_cycles = before.total_cycles;
    let after_cycles = after.total_cycles;
    let before_time_us = before.execution_time_us;
    let after_time_us = after.execution_time_us;
    let speedup_x = if after_cycles > 0 {
        before_cycles as f64 / after_cycles as f64
    } else {
        1.0
    };
    let improvement_pct = (speedup_x - 1.0) * 100.0;

    SpeedupResult {
        before_cycles,
        after_cycles,
        before_time_us,
        after_time_us,
        speedup_x,
        improvement_pct,
    }
}

pub fn compute_diff(original: &str, optimized: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let opt_lines: Vec<&str> = optimized.lines().collect();
    let mut diff = String::new();

    let max_len = orig_lines.len().max(opt_lines.len());
    let mut added = 0;
    let mut removed = 0;

    for i in 0..max_len {
        match (orig_lines.get(i), opt_lines.get(i)) {
            (Some(a), Some(b)) if a != b => {
                diff.push_str(&format!("- {}\n+ {}\n", a, b));
                removed += 1;
                added += 1;
            }
            (Some(a), None) => {
                diff.push_str(&format!("- {}\n", a));
                removed += 1;
            }
            (None, Some(b)) => {
                diff.push_str(&format!("+ {}\n", b));
                added += 1;
            }
            _ => {}
        }
    }

    let summary = format!(
        "{} changes: {} added, {} removed, {} total lines",
        removed + added,
        added,
        removed,
        max_len
    );
    if diff.is_empty() {
        diff = "(no changes)".into();
    } else {
        diff = format!("--- original\n+++ optimized\n{}\n{}", diff, summary);
    }
    diff
}

pub fn build_optimization_prompt(
    source: &str,
    filename: &str,
    language: &GpuLanguage,
    arch: &GpuArch,
    bottleneck: &BottleneckReport,
    gpu_context: &str,
) -> String {
    let specs = emulate::arch_by_enum(*arch);
    let cc = specs.compute_cap;
    let arch_name = specs.name;
    let lang_name = match language {
        GpuLanguage::Cuda => "CUDA",
        GpuLanguage::Triton => "Triton",
        GpuLanguage::Mojo => "Mojo",
        GpuLanguage::Numba => "Numba CUDA",
        GpuLanguage::PyTorch => "PyTorch CUDA",
        GpuLanguage::Cute => "CUTE",
        GpuLanguage::CudaTile => "CUDA Tile",
        GpuLanguage::TileLang => "TileLang",
        GpuLanguage::Unknown => "CUDA",
    };

    let mut details = String::new();
    for d in &bottleneck.details {
        details.push_str(&format!("  - {}\n", d));
    }

    let mut secondary_str = String::new();
    for s in &bottleneck.secondary {
        secondary_str.push_str(&format!("  - {}\n", s));
    }

    format!(
        r##"You are a GPU kernel optimization expert. Optimize the following {lang_name} kernel for {arch_name} (compute capability {cc}).

GPU HARDWARE CONTEXT:
{gpu_context}

BOTTLENECK ANALYSIS:
Primary bottleneck: {primary}
{secondary}

Details:
{details}

SOURCE FILE: {filename}
LANGUAGE: {lang_name}

```{lang}
{source}
```

INSTRUCTIONS:
1. Analyze the kernel for the identified bottlenecks
2. Apply targeted optimizations: tiling for coalescing, shared memory padding for bank conflicts, __launch_bounds__ for register pressure, loop unrolling, vectorized loads
3. Return ONLY the complete rewritten kernel in a code block
4. Add a brief comment at the top explaining what optimizations you applied and why
5. Keep the same function signature (same parameters, same return type)
6. Do NOT change the algorithm — only optimize the implementation

OPTIMIZED KERNEL:
"##,
        lang = lang_name.to_lowercase(),
        lang_name = lang_name,
        gpu_context = gpu_context,
        primary = bottleneck.primary,
        secondary = secondary_str,
        details = details,
        filename = filename,
        source = source,
    )
}

pub fn extract_kernel_from_response(response: &str) -> String {
    if let Some(start) = response.find("```") {
        let after_ticks = &response[start + 3..];
        if let Some(end) = after_ticks.find('\n') {
            let rest = after_ticks[end + 1..].trim();
            if let Some(end_ticks) = rest.find("```") {
                return rest[..end_ticks].trim().to_string();
            }
            return rest.to_string();
        }
    }
    response.trim().to_string()
}

pub fn format_optimize_output(output: &OptimizeOutput) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "╔{}╗", "═".repeat(58));
    let _ = writeln!(out, "║  {:54} ║", "Optimization Results");
    let _ = writeln!(out, "╠{}╣", "═".repeat(58));

    if let Some(ref speedup) = output.speedup_estimate {
        let _color = if speedup.speedup_x >= 2.0 {
            "green"
        } else if speedup.speedup_x >= 1.2 {
            "yellow"
        } else {
            "red"
        };

        let _ = writeln!(out, "║  Before       After        Speedup  {:>28} ║", " ");
        let _ = writeln!(
            out,
            "║  ─────────────────────────────────────────────────  ║"
        );
        let _ = writeln!(
            out,
            "║  {:>10} cy  {:>10} cy  {:>5.2}x ({:>+.0}%)    {:>8} ║",
            speedup.before_cycles,
            speedup.after_cycles,
            speedup.speedup_x,
            speedup.improvement_pct,
            " "
        );
        let _ = writeln!(
            out,
            "║  {:>8.1} μs  {:>8.1} μs  {:>5.2}x                {:>8} ║",
            speedup.before_time_us,
            speedup.after_time_us,
            speedup.before_time_us / speedup.after_time_us.max(0.001),
            " "
        );
        let _ = writeln!(
            out,
            "║                                                     ║"
        );
    }

    let _ = writeln!(out, "║  Bottleneck: {}", output.bottleneck_report.primary);
    for d in &output.bottleneck_report.details {
        let _ = writeln!(out, "║    → {}", d);
    }

    let _ = writeln!(
        out,
        "║                                                     ║"
    );
    match (output.compiled_ok, output.correctness_passed) {
        (Some(true), Some(true)) => {
            let _ = writeln!(
                out,
                "║  ✓ Compilation: PASSED    Correctness: PASSED      ║"
            );
        }
        (Some(false), _) => {
            let _ = writeln!(out, "║  ✗ Compilation: FAILED    ║");
        }
        (Some(true), Some(false)) => {
            let _ = writeln!(
                out,
                "║  ✓ Compilation: PASSED    ✗ Correctness: FAILED    ║"
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "║  ~ Compilation: UNCHECKED  Correctness: UNVERIFIED ║"
            );
        }
    }

    if !output.llm_optimization_notes.is_empty() {
        let _ = writeln!(
            out,
            "║                                                     ║"
        );
        let _ = writeln!(
            out,
            "║  Optimizations applied:                             ║"
        );
        for line in output.llm_optimization_notes.lines() {
            let _ = writeln!(out, "║    {}", line);
        }
    }

    let _ = writeln!(out, "╚{}╝", "═".repeat(58));
    out
}

pub fn format_provider_table(
    cloud: &[(&str, &[(&str, bool)])],
    local: &[(&str, &[(&str, bool)])],
) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "Cloud Providers");
    let _ = writeln!(out, "{}", "─".repeat(50));
    for (name, models) in cloud {
        let status = if models.iter().any(|(_, c)| *c) {
            "✓ ready"
        } else {
            "✗ not configured"
        };
        let _ = writeln!(out, "  ├─ {:<12} {}", name, status);
        for (m, configured) in *models {
            let _ = writeln!(
                out,
                "  │  └─ {:<20} {}",
                m,
                if *configured { "✓" } else { "" }
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Local / Private");
    let _ = writeln!(out, "{}", "─".repeat(50));
    for (name, models) in local {
        let status = if models.iter().any(|(_, c)| *c) {
            "● running"
        } else {
            "○ stopped"
        };
        let _ = writeln!(out, "  ├─ {:<12} {}", name, status);
        for (m, _) in *models {
            let _ = writeln!(out, "  │  └─ {}", m);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_bottlenecks_from_emulate() {
        let source = "__global__ void k(float* a) { int i = blockIdx.x * blockDim.x + threadIdx.x; a[i] = a[i] * 2.0f; }";
        let config = LaunchConfig {
            block_x: 256,
            ..Default::default()
        };
        let result = emulate::emulate(source, &config, &GpuArch::Ampere86);
        let report = analyze_bottlenecks(&result);
        assert!(!report.primary.is_empty());
    }

    #[test]
    fn test_speedup_positive() {
        let source = "__global__ void k(float* a) { int i = threadIdx.x; a[i] *= 2.0f; }";
        let config = LaunchConfig {
            block_x: 256,
            ..Default::default()
        };
        let result = emulate::emulate(source, &config, &GpuArch::Ampere86);
        let speedup = estimate_speedup(&result, &result);
        assert!((speedup.speedup_x - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_speedup_improvement() {
        let source = "__global__ void k(float* a) { int i = threadIdx.x; for(int j=0;j<10;j++) a[i*j] = a[i*j] * 2.0f; }";
        let config_a = LaunchConfig {
            block_x: 32,
            ..Default::default()
        };
        let config_b = LaunchConfig {
            block_x: 64,
            ..Default::default()
        };
        let a = emulate::emulate(source, &config_a, &GpuArch::Ampere86);
        let b = emulate::emulate(source, &config_b, &GpuArch::Ampere86);
        let speedup = estimate_speedup(&a, &b);
        assert!(
            speedup.speedup_x > 0.0,
            "Speedup should be positive, got {}",
            speedup.speedup_x
        );
        assert!(!speedup.speedup_x.is_nan());
    }

    #[test]
    fn test_compute_diff_shows_changes() {
        let orig = "int x = 1;";
        let opt = "int x = 2;";
        let diff = compute_diff(orig, opt);
        assert!(diff.contains("- int x = 1;"));
        assert!(diff.contains("+ int x = 2;"));
    }

    #[test]
    fn test_extract_kernel_from_code_block() {
        let response = "Some text\n```cuda\n__global__ void k() {}\n```\nmore text";
        let kernel = extract_kernel_from_response(response);
        assert_eq!(kernel, "__global__ void k() {}");
    }

    #[test]
    fn test_build_prompt_contains_gpu_context() {
        let source = "__global__ void k() {}";
        let bottle = BottleneckReport {
            primary: "Test",
            secondary: vec![],
            details: vec!["Test detail".into()],
        };
        let prompt = build_optimization_prompt(
            source,
            "k.cu",
            &GpuLanguage::Cuda,
            &GpuArch::Ampere86,
            &bottle,
            "RTX 3090 (sm_86)",
        );
        assert!(prompt.contains("RTX 3090"));
        assert!(prompt.contains("optimization expert"));
    }

    #[test]
    fn test_format_provider_table_output() {
        let cloud_models: &[(&str, bool)] = &[("gpt-4o", true), ("gpt-4o-mini", false)];
        let local_models: &[(&str, bool)] = &[("qwen3:8b", true)];
        let cloud = [("OpenAI", cloud_models)];
        let local = [("Ollama", local_models)];
        let table = format_provider_table(&cloud, &local);
        assert!(table.contains("OpenAI"));
        assert!(table.contains("gpt-4o"));
        assert!(table.contains("Ollama"));
    }

    #[test]
    fn test_extract_kernel_no_code_block() {
        let response = "__global__ void k() { return; }";
        let kernel = extract_kernel_from_response(response);
        assert_eq!(kernel, response);
    }

    #[test]
    fn test_diff_empty_for_identical() {
        let code = "__global__ void k() {}";
        let diff = compute_diff(code, code);
        assert_eq!(diff, "(no changes)");
    }

    #[test]
    fn test_optimize_output_formatting() {
        let report = BottleneckReport {
            primary: "Memory-bound",
            secondary: vec![],
            details: vec!["Test".into()],
        };
        let speedup = SpeedupResult {
            before_cycles: 1000,
            after_cycles: 500,
            before_time_us: 100.0,
            after_time_us: 50.0,
            speedup_x: 2.0,
            improvement_pct: 100.0,
        };
        let out = OptimizeOutput {
            original_source: "orig".into(),
            optimized_source: "opt".into(),
            diff: "test".into(),
            bottleneck_report: report,
            speedup_estimate: Some(speedup),
            llm_optimization_notes: "Tiling applied".into(),
            compiled_ok: Some(true),
            correctness_passed: Some(true),
        };
        let formatted = format_optimize_output(&out);
        assert!(formatted.contains("Optimization Results"));
        assert!(formatted.contains("2.00x"));
        assert!(formatted.contains("PASSED"));
    }

    #[test]
    fn test_analyze_bottleneck_coalescing() {
        let source =
            "__global__ void k(float* a) { int i = threadIdx.x; a[i * 32] = a[i] * 2.0f; }";
        let config = LaunchConfig {
            block_x: 256,
            ..Default::default()
        };
        let result = emulate::emulate(source, &config, &GpuArch::Ampere86);
        let report = analyze_bottlenecks(&result);
        assert!(report.primary.contains("Memory") || report.primary.contains("Compute"));
    }
}
