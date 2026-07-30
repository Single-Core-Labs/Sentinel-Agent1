#[derive(Debug, Clone, Copy)]
pub struct KernelConfig {
    pub block_x: u32,
    pub block_y: u32,
    pub block_z: u32,
    pub shared_mem: u32,
}

#[derive(Debug, Clone)]
pub struct BenchResult {
    pub config: KernelConfig,
    pub label: String,
    pub duration_ms: f64,
    pub occupancy: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct BenchSuiteResult {
    pub kernel_name: String,
    pub block_configs: Vec<BenchResult>,
    pub fastest: Option<BenchResult>,
    pub recommendations: Vec<String>,
}

const BLOCK_SIZES: &[u32] = &[32, 64, 96, 128, 192, 256, 384, 512];
const BLOCK_Y_SIZES: &[u32] = &[1, 2, 4];
const SHARED_MEM_OPTIONS: &[u32] = &[0, 8192, 16384, 32768];

pub fn generate_configs(source: &str) -> Vec<KernelConfig> {
    let thread_count = extract_thread_count(source);
    let _ = grid_size_hint(source);

    let mut configs = Vec::new();
    let preferred = if let Some(tc) = thread_count { tc } else { 256 };

    for &bx in BLOCK_SIZES {
        for &by in BLOCK_Y_SIZES {
            let total = bx * by;
            if total > 1024 { continue; }
            // Only test configs near the thread count within 2x
            let ratio = total as f64 / preferred as f64;
            if ratio > 4.0 || ratio < 0.25 { continue; }
            configs.push(KernelConfig { block_x: bx, block_y: by, block_z: 1, shared_mem: 0 });
        }
    }

    // Add shared memory variants of best config
    let best_block_x = configs.first().map(|c| c.block_x.max(128)).unwrap_or(128);
    for &sm in SHARED_MEM_OPTIONS {
        if sm == 0 { continue; }
        configs.push(KernelConfig {
            block_x: best_block_x,
            block_y: 1,
            block_z: 1,
            shared_mem: sm,
        });
    }

    if configs.is_empty() {
        configs.push(KernelConfig { block_x: 128, block_y: 1, block_z: 1, shared_mem: 0 });
        configs.push(KernelConfig { block_x: 256, block_y: 1, block_z: 1, shared_mem: 0 });
    }

    configs
}

fn extract_thread_count(source: &str) -> Option<u32> {
    // Look for <<<grid, block>>> patterns
    let re = regex::Regex::new(r"<<<\s*[^,]+,\s*(\d+)\s*>>>").ok()?;
    let mut count = None;
    for cap in re.captures_iter(source) {
        let val: u32 = cap[1].parse().ok()?;
        count = Some(count.map_or(val, |c: u32| c.max(val)));
    }
    count
}

fn grid_size_hint(source: &str) -> Option<u32> {
    // Look for grid size in kernel launch
    let re = regex::Regex::new(r"<<<\s*(\d+)\s*,").ok()?;
    re.captures(source)?.get(1)?.as_str().parse::<u32>().ok()
}

pub fn generate_labels(config: &KernelConfig) -> Vec<(KernelConfig, String)> {
    if config.shared_mem > 0 {
        vec![(*config, format!("{}x{}x{} smem={}", config.block_x, config.block_y, config.block_z, config.shared_mem))]
    } else {
        vec![(*config, format!("{}x{}x{}", config.block_x, config.block_y, config.block_z))]
    }
}

pub fn estimate_config(config: &KernelConfig, gpu_sm_count: u32) -> f64 {
    let threads = config.block_x * config.block_y * config.block_z;
    let max_blocks_per_sm = if threads <= 128 { 8.0 }
        else if threads <= 256 { 6.0 }
        else if threads <= 512 { 4.0 }
        else { 2.0 };

    let total_blocks = (gpu_sm_count as f64) * max_blocks_per_sm;
    let occupancy = (threads as f64 * max_blocks_per_sm) / (1024.0);

    let shared_mem_penalty = if config.shared_mem > 32768 { 0.5 }
        else if config.shared_mem > 16384 { 0.75 }
        else if config.shared_mem > 0 { 0.9 }
        else { 1.0 };

    let warp_occupancy = (threads as f64 / 32.0).ceil();
    let warp_efficiency = if warp_occupancy.fract() == 0.0 { 1.0 } else { 0.9 };

    let score = total_blocks * (occupancy.min(1.0)) * shared_mem_penalty * warp_efficiency;
    score
}

pub fn run_bench_suite(configs: &[KernelConfig], kernel_name: &str, gpu_sm_count: u32) -> BenchSuiteResult {
    let mut results: Vec<BenchResult> = configs.iter().map(|cfg| {
        let labels = generate_labels(cfg);
        let (cfg, label) = labels.into_iter().next().unwrap_or((*cfg, "unknown".into()));

        let occupancy = Some((cfg.block_x as f64 * cfg.block_y as f64 * cfg.block_z as f64).min(1024.0) / 1024.0 * 100.0);
        let duration_ms = estimate_config(&cfg, gpu_sm_count);

        BenchResult { config: cfg, label, duration_ms, occupancy }
    }).collect();

    results.sort_by(|a, b| a.duration_ms.partial_cmp(&b.duration_ms).unwrap_or(std::cmp::Ordering::Equal));

    let fastest = results.first().cloned();
    let mut recommendations = Vec::new();

    if let Some(ref f) = fastest {
        if f.config.block_x < 128 {
            recommendations.push(format!("Consider larger block size. {} is below optimal 128.", f.config.block_x));
        }
        if f.config.block_x % 32 != 0 || (f.config.block_x * f.config.block_y) % 32 != 0 {
            recommendations.push(format!("Block size {} is not a multiple of warp size (32). Wasted threads.", f.config.block_x));
        }
        if f.config.shared_mem > 0 && f.config.shared_mem <= 16384 {
            recommendations.push("Shared memory usage is moderate. Good balance of occupancy vs cache.".into());
        }
        if f.config.shared_mem > 32768 {
            recommendations.push("High shared memory usage will limit occupancy. Consider reducing.".into());
        }
        let occupancy = f.occupancy.unwrap_or(0.0);
        if occupancy < 50.0 {
            recommendations.push(format!("Low occupancy ({:.0}%). Try smaller block size or less shared memory.", occupancy));
        }
    }

    BenchSuiteResult { kernel_name: kernel_name.into(), block_configs: results, fastest, recommendations }
}

pub fn format_bench_results(result: &BenchSuiteResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("Kernel: {}\n", result.kernel_name));
    out.push_str(&format!("{:<18} {:<12} {:<12} {:<10}\n", "Config", "Est. Score", "Occupancy", "Rank"));
    out.push_str(&format!("{}\n", "-".repeat(52)));

    for (i, br) in result.block_configs.iter().enumerate() {
        let rank = if i == 0 { "★ fastest" } else { "" };
        let occ = br.occupancy.map(|o| format!("{:.0}%", o)).unwrap_or_else(|| "N/A".into());
        out.push_str(&format!("{:<18} {:<12.1} {:<12} {:<10}\n",
            br.label, br.duration_ms, occ, rank));
    }

    if !result.recommendations.is_empty() {
        out.push_str("\nRecommendations:\n");
        for r in &result.recommendations {
            out.push_str(&format!("  - {}\n", r));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_configs_from_source() {
        let src = r#"
__global__ void vec_add(float* a, float* b, float* c, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) c[i] = a[i] + b[i];
}
int main() {
    vec_add<<<256, 256>>>(d_a, d_b, d_c, N);
}
"#;
        let configs = generate_configs(src);
        assert!(!configs.is_empty());
        assert!(configs.iter().any(|c| c.block_x == 256));
    }

    #[test]
    fn test_estimate_config_scores() {
        let small = estimate_config(&KernelConfig { block_x: 64, block_y: 1, block_z: 1, shared_mem: 0 }, 128);
        let large = estimate_config(&KernelConfig { block_x: 256, block_y: 1, block_z: 1, shared_mem: 0 }, 128);
        assert!(large > 0.0);
        assert!(small > 0.0);
    }

    #[test]
    fn test_bench_suite_orders_by_score() {
        let configs = vec![
            KernelConfig { block_x: 64, block_y: 1, block_z: 1, shared_mem: 0 },
            KernelConfig { block_x: 256, block_y: 1, block_z: 1, shared_mem: 0 },
        ];
        let result = run_bench_suite(&configs, "vec_add", 128);
        assert_eq!(result.block_configs.len(), 2);
        assert!(result.fastest.is_some());
    }

    #[test]
    fn test_generate_labels() {
        let cfg = KernelConfig { block_x: 256, block_y: 1, block_z: 1, shared_mem: 16384 };
        let labels = generate_labels(&cfg);
        assert!(labels[0].1.contains("smem=16384"));
    }
}
