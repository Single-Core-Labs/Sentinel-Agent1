use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone)]
pub struct KernelIssue {
    pub line: usize,
    pub severity: Severity,
    pub message: String,
    pub suggestion: String,
}

pub fn analyze_cuda_source(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    detect_divergent_sync(&lines, &mut issues);
    detect_shared_mem_oversubscription(&lines, &mut issues);
    detect_small_block_size(&lines, &mut issues);
    detect_atomic_in_loop(&lines, &mut issues);
    detect_host_device_copy(&lines, &mut issues);
    detect_uncoalesced_access(&lines, &mut issues);
    detect_bank_conflicts(&lines, &mut issues);

    issues
}

fn detect_divergent_sync(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    let mut in_if = false;
    let mut depth = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if (trimmed.starts_with("if") && trimmed.contains("__syncthreads")
            || trimmed.starts_with("if"))
            && (trimmed.ends_with('{') || trimmed.contains('{'))
        {
            in_if = true;
            depth = 1;
        }
        if in_if {
            if trimmed.contains('{') {
                depth += 1;
            }
            if trimmed.contains('}') {
                depth -= 1;
            }
            if trimmed.contains("__syncthreads()") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Error,
                    message: "Divergent __syncthreads() call inside conditional block".into(),
                    suggestion: "Move __syncthreads() outside the if/else block. All threads in a block must execute __syncthreads() to avoid deadlock.".into(),
                });
            }
            if depth <= 0 {
                in_if = false;
            }
        }
    }
}

fn detect_shared_mem_oversubscription(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    // Match: __shared__ float arr[N] or __shared__ int arr[M]
    let re = Regex::new(r"__shared__\s+\w+\s+\w+\[(\d+)\]").unwrap();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.contains("__shared__") {
            continue;
        }

        if let Some(caps) = re.captures(trimmed) {
            if let Ok(size) = caps[1].parse::<usize>() {
                let element_size = if trimmed.contains("double") { 8 } else { 4 };
                let bytes = size * element_size;

                if bytes > 48 * 1024 {
                    issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Warn,
                        message: format!("Shared memory oversubscription: {} bytes (exceeds 48KB limit)", bytes),
                        suggestion: "Reduce shared memory allocation, use dynamic allocation with extern __shared__, or restructure kernel to use registers/global memory.".into(),
                    });
                } else if bytes > 32 * 1024 {
                    issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Info,
                        message: format!("Shared memory usage: {} bytes (moderate)", bytes),
                        suggestion: "Monitor occupancy. >32KB shared memory reduces max blocks per SM on most GPUs.".into(),
                    });
                }
            }
        }
    }
}

fn detect_small_block_size(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    let global_re = Regex::new(r"<<<\s*([^,]+)\s*,\s*([^>]+)\s*>>>").unwrap();
    let dim_re = Regex::new(r"dim3\s*\(\s*(\d+)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if let Some(caps) = global_re.captures(trimmed) {
            let grid_str = caps[1].trim();
            let block_str = caps[2].trim();

            let block_x = if let Some(dc) = dim_re.captures(block_str) {
                dc[1].parse::<u32>().unwrap_or(256)
            } else {
                block_str
                    .split(',')
                    .next()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(256)
            };

            if block_x < 32 {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: format!("Small block size: {} threads (less than warp size 32)", block_x),
                    suggestion: "Increase blockDim.x to at least 32 to avoid underutilized warps. Target 128-256 threads per block.".into(),
                });
            }

            let grid_x = if let Some(dc) = dim_re.captures(grid_str) {
                dc[1].parse::<u32>().unwrap_or(1)
            } else {
                grid_str
                    .split(',')
                    .next()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(1)
            };

            if grid_x < 80 {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: format!("Low grid size: {} blocks (may underutilize GPU)", grid_x),
                    suggestion: "Increase grid size to at least 80 blocks to keep all SMs busy. Target 4-8 blocks per SM.".into(),
                });
            }
        }
    }
}

fn detect_atomic_in_loop(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    let mut in_loop = false;
    let mut depth = 0;

    let atomic_ops = [
        "atomicAdd",
        "atomicSub",
        "atomicExch",
        "atomicMin",
        "atomicMax",
        "atomicAnd",
        "atomicOr",
        "atomicXor",
        "atomicCAS",
    ];

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if (trimmed.starts_with("for") || trimmed.starts_with("while")) && trimmed.contains('{') {
            in_loop = true;
            depth = 1;
        }
        if in_loop {
            if trimmed.contains('{') {
                depth += 1;
            }
            if trimmed.contains('}') {
                depth -= 1;
            }

            for &op in &atomic_ops {
                if trimmed.contains(op) {
                    issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Warn,
                        message: format!("{} inside loop: serialized contention on every iteration", op),
                        suggestion: "Move atomic operation outside loop, use warp-level reduction (__shfl_down_sync), or batch updates with shared memory.".into(),
                    });
                }
            }

            if depth <= 0 {
                in_loop = false;
            }
        }
    }
}

fn detect_host_device_copy(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("cudaMemcpy") {
            let in_kernel = lines[..=i]
                .iter()
                .any(|l| l.trim().contains("__global__") || l.trim().contains("__device__"));

            if in_kernel {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Error,
                    message: "cudaMemcpy called from device code".into(),
                    suggestion: "cudaMemcpy is a host-side API. Use __ldg() for read-only global memory access or copy data before kernel launch.".into(),
                });
            }
        }
    }
}

fn detect_uncoalesced_access(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    // Pattern: arr[threadIdx.x][blockIdx.x]  — tiled access (coalesced)
    // Pattern: arr[blockIdx.x][threadIdx.x]  — strided access (non-coalesced)
    let re = Regex::new(r"(\w+)\[(\w+)\]\[(\w+)\]").unwrap();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        if let Some(caps) = re.captures(trimmed) {
            let first = &caps[2];
            let second = &caps[3];

            if first.starts_with("blockIdx")
                && (second.starts_with("threadIdx") || second.starts_with("laneId"))
                && !trimmed.contains("//")
                && !trimmed.starts_with("//")
            {
                issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Warn,
                        message: "Potentially uncoalesced global memory access pattern".into(),
                        suggestion: "Use [threadIdx.x][blockIdx.x] instead of [blockIdx.x][threadIdx.x] for coalesced access. Transpose or use shared memory to reorder.".into(),
                    });
            }
        }
    }
}

fn detect_bank_conflicts(lines: &[&str], issues: &mut Vec<KernelIssue>) {
    // Detect shared memory arrays accessed with strided indices
    let re = Regex::new(r"shared_?\w*\[(\w+)\s*\+\s*(\w+)\]").unwrap();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.contains("__shared__") {
            continue;
        }

        for caps in re.captures_iter(trimmed) {
            let base = &caps[1];
            let idx = &caps[2];

            // Simple heuristic: if idx is a multiple of warp-like stride
            if idx.contains("*") || idx.contains("32") || idx.contains("64") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: format!("Potential bank conflict in shared memory access pattern ({} + {})", base, idx),
                    suggestion: "Pad shared memory array by 1 element per row: __shared__ float shared[N][M+1]. Use __launch_bounds__ to control block size.".into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_divergent_sync() {
        let src = r#"
__global__ void test() {
    if (threadIdx.x < 16) {
        __syncthreads();
    }
}"#;
        let issues = analyze_cuda_source(src);
        assert!(issues.iter().any(|i| i.message.contains("Divergent")));
    }

    #[test]
    fn test_shared_mem_oversubscription() {
        let src = "__shared__ float big[131072];";
        let issues = analyze_cuda_source(src);
        assert!(issues
            .iter()
            .any(|i| i.message.contains("oversubscription")));
    }

    #[test]
    fn test_small_block() {
        let src = "kernel<<<1, 16>>>();";
        let issues = analyze_cuda_source(src);
        assert!(issues
            .iter()
            .any(|i| i.message.contains("Small block size")));
    }

    #[test]
    fn test_atomic_in_loop() {
        let src = r#"
for (int i = 0; i < N; i++) {
    atomicAdd(&result, data[i]);
}"#;
        let issues = analyze_cuda_source(src);
        assert!(issues
            .iter()
            .any(|i| i.message.contains("atomicAdd inside loop")));
    }

    #[test]
    fn test_host_device_copy() {
        let src = "__global__ void test() { cudaMemcpy(dst, src, size, cudaMemcpyDeviceToHost); }";
        let issues = analyze_cuda_source(src);
        assert!(issues
            .iter()
            .any(|i| i.message.contains("cudaMemcpy called from device")));
    }
}
