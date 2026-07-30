use crate::cuda::{self, KernelIssue, Severity};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpuLanguage {
    Cuda,
    Triton,
    Mojo,
    Numba,
    PyTorch,
    Cute,
    CudaTile,
    TileLang,
    Unknown,
}

impl GpuLanguage {
    pub fn name(&self) -> &'static str {
        match self {
            GpuLanguage::Cuda => "CUDA C/C++",
            GpuLanguage::Triton => "OpenAI Triton",
            GpuLanguage::Mojo => "Mojo",
            GpuLanguage::Numba => "Numba CUDA",
            GpuLanguage::PyTorch => "PyTorch",
            GpuLanguage::Cute => "CUTE (CUTLASS Templates)",
            GpuLanguage::CudaTile => "CUDA Tile",
            GpuLanguage::TileLang => "TileLang",
            GpuLanguage::Unknown => "Unknown",
        }
    }

    pub fn file_extensions(&self) -> &[&'static str] {
        match self {
            GpuLanguage::Cuda => &["cu", "cuh", "cuda"],
            GpuLanguage::Triton => &["triton.py", "py"],
            GpuLanguage::Mojo => &["mojo", "🔥"],
            GpuLanguage::Numba => &["py"],
            GpuLanguage::PyTorch => &["py"],
            GpuLanguage::Cute => &["hpp", "h", "cuh"],
            GpuLanguage::CudaTile => &["cu", "cuh"],
            GpuLanguage::TileLang => &["py", "tile"],
            GpuLanguage::Unknown => &[],
        }
    }
}

pub struct AnalysisResult {
    pub language: GpuLanguage,
    pub issues: Vec<KernelIssue>,
    pub config_hint: String,
}

pub fn detect_language(filename: &str, source: &str) -> GpuLanguage {
    let lower = filename.to_lowercase();

    // Check by extension first
    if lower.ends_with(".cu") || lower.ends_with(".cuh") {
        if source.contains("CUTE_DEVICE") || source.contains("CUTE_HOST") || source.contains("Cutlass") {
            return GpuLanguage::Cute;
        }
        if source.contains("TiledMMA") || source.contains("Swizzle") || source.contains("cute::") {
            return GpuLanguage::Cute;
        }
        if source.contains("TILE_DIM") || source.contains("tile_load") || source.contains("tile_store") {
            return GpuLanguage::CudaTile;
        }
        return GpuLanguage::Cuda;
    }

    if lower.ends_with(".mojo") || lower.ends_with("🔥") {
        return GpuLanguage::Mojo;
    }

    // Check content patterns for Python-based DSLs
    if source.contains("@triton.jit") || source.contains("import triton") || source.contains("from triton") {
        return GpuLanguage::Triton;
    }

    if source.contains("@cuda.jit") || source.contains("from numba import cuda") || source.contains("numba.cuda") {
        return GpuLanguage::Numba;
    }

    if source.contains("torch.compile") && (source.contains("cuda") || source.contains("CUDA")) {
        return GpuLanguage::PyTorch;
    }

    if lower.ends_with(".py") && (source.contains("torch.cuda") || source.contains("torch.Tensor") || source.contains(".cuda()") || source.contains("import torch") || source.contains("from torch")) {
        return GpuLanguage::PyTorch;
    }

    if source.contains("tilelang") || source.contains("TileLang") {
        return GpuLanguage::TileLang;
    }

    GpuLanguage::Cuda
}

pub fn analyze(filename: &str, source: &str) -> AnalysisResult {
    let language = detect_language(filename, source);
    let issues = match language {
        GpuLanguage::Cuda => cuda::analyze_cuda_source(source),
        GpuLanguage::Triton => analyze_triton(source),
        GpuLanguage::Mojo => analyze_mojo(source),
        GpuLanguage::Numba => analyze_numba(source),
        GpuLanguage::PyTorch => analyze_pytorch(source),
        GpuLanguage::Cute => analyze_cute(source),
        GpuLanguage::CudaTile => analyze_cuda_tile(source),
        GpuLanguage::TileLang => analyze_tilelang(source),
        GpuLanguage::Unknown => cuda::analyze_cuda_source(source),
    };

    let config_hint = config_hint_for(language);

    AnalysisResult { language, issues, config_hint }
}

fn config_hint_for(lang: GpuLanguage) -> String {
    match lang {
        GpuLanguage::Cuda => "NVCC flags: -arch=sm_86 (RTX 30xx) / -arch=sm_89 (RTX 40xx) / -arch=sm_90 (H100)".into(),
        GpuLanguage::Triton => "Triton autotune: @triton.autotune(configs=[triton.Config({'BLOCK_M': 128, 'BLOCK_N': 128}, num_warps=4)])".into(),
        GpuLanguage::Mojo => "Mojo compile: mojo build --target cuda <file>.mojo".into(),
        GpuLanguage::Numba => "Numba config: numba.cuda.set_config('compile_threads', 4)".into(),
        GpuLanguage::PyTorch => "torch.compile: torch.compile(model, mode='max-autotune', backend='inductor')".into(),
        GpuLanguage::Cute => "CUTE compile: nvcc -arch=sm_90 -DCUTE_ARCH_SM90 <file>.cu".into(),
        GpuLanguage::CudaTile => "Tile NVCC: nvcc -arch=sm_86 --use_fast_math <file>.cu".into(),
        GpuLanguage::TileLang => "TileLang: tilelang.compile(kernel, target='cuda')".into(),
        GpuLanguage::Unknown => "Consider using CUDA, Triton, or Numba for GPU kernels".into(),
    }
}

pub struct BlockSizeRecommendation {
    pub label: &'static str,
    pub block_x: u32,
    pub block_y: u32,
    pub block_z: u32,
    pub shared_mem: u32,
    pub reason: &'static str,
}

pub fn recommended_block_sizes(lang: GpuLanguage, compute_capability: Option<&str>) -> Vec<BlockSizeRecommendation> {
    let cc_major: u32 = compute_capability
        .and_then(|s| s.split('.').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    match lang {
        GpuLanguage::Cuda => {
            if cc_major >= 9 {
                vec![
                    BlockSizeRecommendation { label: "Hopper-optimized", block_x: 128, block_y: 2, block_z: 1, shared_mem: 0, reason: "Best occupancy on SM90 with 128 threads/warpgroup" },
                    BlockSizeRecommendation { label: "Large shared mem", block_x: 128, block_y: 1, block_z: 1, shared_mem: 49152, reason: "TMA + shared memory for Hopper tensor cores" },
                    BlockSizeRecommendation { label: "Max occupancy", block_x: 256, block_y: 1, block_z: 1, shared_mem: 0, reason: "High occupancy for compute-bound kernels" },
                ]
            } else if cc_major >= 8 {
                vec![
                    BlockSizeRecommendation { label: "Ampere-optimized", block_x: 128, block_y: 2, block_z: 1, shared_mem: 0, reason: "Best balance on SM86/87, 256 threads per block" },
                    BlockSizeRecommendation { label: "Max throughput", block_x: 256, block_y: 1, block_z: 1, shared_mem: 0, reason: "64 warps/SM, hides latency well" },
                    BlockSizeRecommendation { label: "Shared-mem heavy", block_x: 128, block_y: 1, block_z: 1, shared_mem: 32768, reason: "32KB shared mem for matmul/tiling" },
                ]
            } else {
                vec![
                    BlockSizeRecommendation { label: "Turing-optimized", block_x: 128, block_y: 1, block_z: 1, shared_mem: 0, reason: "128 threads/SM optimal on SM75" },
                    BlockSizeRecommendation { label: "Occupancy max", block_x: 256, block_y: 1, block_z: 1, shared_mem: 0, reason: "64 warps, good for memory-bound kernels" },
                ]
            }
        }
        GpuLanguage::Triton => {
            vec![
                BlockSizeRecommendation { label: "Triton default", block_x: 128, block_y: 128, block_z: 1, shared_mem: 0, reason: "Standard BLOCK_M=128, BLOCK_N=128 for matmul" },
                BlockSizeRecommendation { label: "Large tiles", block_x: 256, block_y: 256, block_z: 1, shared_mem: 0, reason: "Higher arithmetic intensity for large matrices" },
                BlockSizeRecommendation { label: "Small tiles", block_x: 64, block_y: 64, block_z: 1, shared_mem: 0, reason: "Lower register pressure, good for small N" },
            ]
        }
        GpuLanguage::Mojo | GpuLanguage::Numba => {
            vec![
                BlockSizeRecommendation { label: "Python-default", block_x: 256, block_y: 1, block_z: 1, shared_mem: 0, reason: "256 threads/block, good for most kernels" },
                BlockSizeRecommendation { label: "2D tiling", block_x: 16, block_y: 16, block_z: 1, shared_mem: 0, reason: "16x16 for 2D grid workloads" },
                BlockSizeRecommendation { label: "Warp-optimized", block_x: 128, block_y: 2, block_z: 1, shared_mem: 0, reason: "128 threads, 4 warps, low divergence" },
            ]
        }
        GpuLanguage::PyTorch => {
            vec![
                BlockSizeRecommendation { label: "torch.compile", block_x: 0, block_y: 0, block_z: 0, shared_mem: 0, reason: "Let inductor choose best block size via autotune" },
                BlockSizeRecommendation { label: "CUDA graphs", block_x: 0, block_y: 0, block_z: 0, shared_mem: 0, reason: "Enable CUDA graphs for static workloads" },
            ]
        }
        GpuLanguage::Cute | GpuLanguage::CudaTile => {
            vec![
                BlockSizeRecommendation { label: "CUTE warp tile", block_x: 64, block_y: 2, block_z: 1, shared_mem: 0, reason: "CUTE warp-per-tile for tensor cores" },
                BlockSizeRecommendation { label: "Large MMA tile", block_x: 128, block_y: 1, block_z: 1, shared_mem: 32768, reason: "32KB shared mem, 128 threads for MMA" },
            ]
        }
        GpuLanguage::TileLang => {
            vec![
                BlockSizeRecommendation { label: "TileLang auto", block_x: 128, block_y: 128, block_z: 1, shared_mem: 0, reason: "Let TileLang JIT choose tile sizes" },
            ]
        }
        GpuLanguage::Unknown => {
            vec![
                BlockSizeRecommendation { label: "Generic", block_x: 256, block_y: 1, block_z: 1, shared_mem: 0, reason: "Safe default for unknown GPU kernel" },
            ]
        }
    }
}

// ── Triton Analysis ──

fn analyze_triton(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Detect missing @triton.autotune
    let has_autotune = source.contains("@triton.autotune");
    let has_jit = source.contains("@triton.jit");

    if has_jit && !has_autotune {
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == "@triton.jit" {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: "Missing @triton.autotune for this kernel".into(),
                    suggestion: "Add @triton.autotune decorator with BLOCK_SIZE configs to automatically select the best block size.".into(),
                });
                break;
            }
        }
    }

    // Detect hardcoded block sizes
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let re = regex::Regex::new(r#"BLOCK_(?:M|N|K|SIZE)\s*=\s*(\d+)"#).unwrap();
        if let Some(caps) = re.captures(trimmed) {
            if let Ok(size) = caps[1].parse::<u32>() {
                if !has_autotune && (size < 32 || size > 512) {
                    issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Warn,
                        message: format!("Static BLOCK_SIZE = {} without autotune", size),
                        suggestion: "Use @triton.autotune to sweep BLOCK_SIZE values (64, 128, 256) at compile time.".into(),
                    });
                }
                if !(size.is_power_of_two()) {
                    issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Warn,
                        message: format!("BLOCK_SIZE {} is not a power of 2", size),
                        suggestion: "Triton BLOCK_SIZE should be a power of 2 for optimal memory coalescing.".into(),
                    });
                }
            }
        }
    }

    // Detect num_warps usage
    if has_jit {
        let has_num_warps = source.contains("num_warps");
        if !has_num_warps && !has_autotune {
            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "@triton.jit" {
                    issues.push(KernelIssue {
                        line: i + 1,
                        severity: Severity::Info,
                        message: "No num_warps specified, using default (4)".into(),
                        suggestion: "Specify num_warps=4, 8, or 16 based on register pressure and shared memory usage.".into(),
                    });
                    break;
                }
            }
        }
    }

    // Detect tl.atomic_add patterns without masking
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("tl.atomic_add") && trimmed.contains("tl.full") {
            issues.push(KernelIssue {
                line: i + 1,
                severity: Severity::Info,
                message: "Potential redundant atomic_add with full mask".into(),
                suggestion: "Use tl.atomic_add with a proper predicate mask to avoid unnecessary atomic operations.".into(),
            });
        }
    }

    issues
}

// ── Mojo Analysis ──

fn analyze_mojo(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_kernel = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect @parameter kernel
        if trimmed.contains("@parameter") && (trimmed.contains("fn") || i + 1 < lines.len() && lines[i + 1].contains("fn")) {
            in_kernel = true;
        }

        // Detect tensor_load without alignment
        if trimmed.contains("tensor_load") || trimmed.contains("simt_load") {
            if !source.contains("aligned") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: "Unaligned memory load in Mojo kernel".into(),
                    suggestion: "Use aligned variants (aligned=True or align_by) for coalesced memory access.".into(),
                });
            }
        }

        // Detect vectorize hint
        if trimmed.contains("vectorize") || trimmed.contains("vector_width") {
            // Good, vectorization is explicit
        }

        // Detect unrolled loops in kernels
        if in_kernel && (trimmed.contains("for") || trimmed.contains("while")) {
            if !source.contains("unroll") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: "Loop without @unroll in kernel".into(),
                    suggestion: "Use @unroll decorator on small loops to eliminate loop overhead in GPU kernels.".into(),
                });
            }
        }
    }

    issues
}

// ── Numba Analysis ──

fn analyze_numba(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_kernel = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.contains("@cuda.jit") || trimmed == "@cuda.jit()" {
            in_kernel = true;
        }

        if in_kernel && trimmed.starts_with("def ") {
            // Check for fastmath
            let kernel_def = trimmed;

            // Check for explicit shared memory
            let has_shared = source.contains("cuda.shared.array") || source.contains("shared.array");

            // Check for block/thread indexing
            let has_cuda_grid = source.contains("cuda.grid") || source.contains("cuda.block") || source.contains("cuda.thread");

            if !has_cuda_grid {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: "Missing cuda.grid/cuda.thread indexing".into(),
                    suggestion: "Use cuda.grid(2) or cuda.threadIdx/cuda.blockIdx for proper GPU thread indexing.".into(),
                });
            }

            if !has_shared && source.contains("reduction") || source.contains("sum") || source.contains("histogram") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: "Reduction kernel without shared memory".into(),
                    suggestion: "Use cuda.shared.array to store per-block partial results and reduce global memory traffic.".into(),
                });
            }

            if i > 0 {
                let prev_line = lines[i - 1].trim();
                if prev_line.contains("@cuda.jit") && !kernel_def.contains("fastmath") {
                    issues.push(KernelIssue {
                        line: i,
                        severity: Severity::Warn,
                        message: "Kernel without fastmath=True".into(),
                        suggestion: "Add fastmath=True to @cuda.jit for ~1.5x speedup on float operations.".into(),
                    });
                }
            }
        }
    }

    // Check for cuda.to_device / cuda.copy_host_to_device in hot path
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("cuda.to_device") || trimmed.contains("cuda.copy_host_to_device") {
            if trimmed.starts_with("for") || source[..i].contains("for") || source[..i].contains("while") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Error,
                    message: "Host-device copy inside loop".into(),
                    suggestion: "Move cuda.to_device outside the loop. Pre-allocate device arrays and reuse.".into(),
                });
            }
        }
    }

    issues
}

// ── PyTorch Analysis ──

fn analyze_pytorch(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect missing torch.compile
        if trimmed.starts_with("model") && trimmed.contains(".cuda()") {
            let has_compile = source.contains("torch.compile");
            if !has_compile {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: "Model moved to CUDA without torch.compile".into(),
                    suggestion: "Wrap model with torch.compile(model, mode='reduce-overhead') for 2-3x inference speedup.".into(),
                });
            }
        }

        // Detect .item() or .cpu() in training loop
        if (trimmed.contains(".item()") || trimmed.contains(".cpu()")) && source.contains("for ") && (source.contains("range(") || source.contains("epoch")) {
            issues.push(KernelIssue {
                line: i + 1,
                severity: Severity::Warn,
                message: "Device synchronization in training loop (from .item() or .cpu())".into(),
                suggestion: "Avoid .item()/.cpu() in loops. Use .detach() and accumulate on GPU. Move syncs outside the loop.".into(),
            });
        }

        // Detect missing pin_memory
        if trimmed.contains("DataLoader") && !source.contains("pin_memory") {
            issues.push(KernelIssue {
                line: i + 1,
                severity: Severity::Info,
                message: "DataLoader without pin_memory=True".into(),
                suggestion: "Add pin_memory=True to DataLoader for faster CPU-to-GPU transfer with non_blocking=True.".into(),
            });
        }

        // Detect CUDA graph opportunity
        if source.contains("for") && source.contains("range") && source.contains("forward") || source.contains("__call__") {
            // Check if graph already used
            if !source.contains("cuda.CUDAGraph") && !source.contains("make_graph") {
                for (j, l) in lines.iter().enumerate() {
                    if l.contains("for") && (l.contains("epoch") || l.contains("iter")) {
                        issues.push(KernelIssue {
                            line: j + 1,
                            severity: Severity::Info,
                            message: "Repeated forward pass — CUDA Graph opportunity".into(),
                            suggestion: "Capture the model forward pass with torch.cuda.CUDAGraph for fixed-shape workloads.".into(),
                        });
                        break;
                    }
                }
            }
        }

        // Detect gradient accumulation without no_grad
        if trimmed.contains("backward()") && source.contains("optimizer.step()") {
            if source.contains("for") {
                for (j, l) in lines.iter().enumerate() {
                    if l.contains("for") {
                        if j < i && !source.contains("no_grad") {
                            issues.push(KernelIssue {
                                line: j + 1,
                                severity: Severity::Info,
                                message: "Gradient accumulation without @torch.no_grad() on inference steps".into(),
                                suggestion: "Wrap non-training passes with @torch.no_grad() to save GPU memory and compute.".into(),
                            });
                            break;
                        }
                    }
                }
            }
        }
    }

    issues
}

// ── CUTE (CUTLASS Templates) Analysis ──

fn analyze_cute(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Check for CUTE_ARCH macro
        let has_arch = source.contains("CUTE_ARCH_SM") || source.contains("CUTE_ARCH") && source.contains("SM");
        if !has_arch && (trimmed.contains("CUTE_DEVICE") || trimmed.contains("__global__")) && trimmed.contains("TiledMMA") || trimmed.contains("GMMA") {
            issues.push(KernelIssue {
                line: i + 1,
                severity: Severity::Error,
                message: "Missing CUTE architecture macro".into(),
                suggestion: "Define -DCUTE_ARCH_SM90 (H100) or -DCUTE_ARCH_SM89 (RTX 40xx) at compile time.".into(),
            });
        }

        // Detect TiledMMA without swizzle
        if trimmed.contains("TiledMMA") {
            let has_swizzle = source.contains("Swizzle") || source.contains("Swizzle<");
            if !has_swizzle {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: "TiledMMA without Swizzle".into(),
                    suggestion: "Add Swizzle<B, M, S> to your TiledMMA for shared memory bank conflict avoidance.".into(),
                });
            }
        }

        // Detect copy_atom vs. SM90 GMMA
        if trimmed.contains("Copy_Atom") || trimmed.contains("copy_atom") {
            if source.contains("CUTE_ARCH_SM90") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: "Using Copy_Atom on SM90 — GMMA is preferred".into(),
                    suggestion: "On Hopper (SM90), use GMMA::MMA_Atom over Copy_Atom for 2x Tensor Core throughput.".into(),
                });
            }
        }

        // Check for MMA traits
        if trimmed.contains("MMA_Atom") || trimmed.contains("TiledMMA") {
            let has_atom = trimmed.contains("Atom") || trimmed.contains("Traits");
            if !has_atom && trimmed.contains("TiledMMA") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: "TiledMMA without explicit Atom/Traits".into(),
                    suggestion: "Specify MMA_Atom<OP, T> template parameters explicitly to avoid default sub-optimal configs.".into(),
                });
            }
        }
    }

    issues
}

// ── CUDA Tile Analysis ──

fn analyze_cuda_tile(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect TILE_DIM
        if trimmed.starts_with("#define") && trimmed.contains("TILE_DIM") {
            let re = regex::Regex::new(r"TILE_DIM\s+(\d+)").unwrap();
            if let Some(caps) = re.captures(trimmed) {
                if let Ok(dim) = caps[1].parse::<u32>() {
                    if dim < 16 || dim > 128 {
                        issues.push(KernelIssue {
                            line: i + 1,
                            severity: Severity::Warn,
                            message: format!("TILE_DIM = {} outside recommended range [16, 128]", dim),
                            suggestion: "Tile dimensions should match warp size (32) or be a multiple of 32 for coalesced access.".into(),
                        });
                    }
                    if dim % 32 != 0 {
                        issues.push(KernelIssue {
                            line: i + 1,
                            severity: Severity::Warn,
                            message: format!("TILE_DIM = {} not a multiple of warp size (32)", dim),
                            suggestion: "Tile dimensions should be multiples of 32 to avoid partial warp waste.".into(),
                        });
                    }
                }
            }
        }

        // Detect tile_load without vector type
        if trimmed.contains("tile_load") || trimmed.contains("tile_store") && !trimmed.contains("vector") && !trimmed.contains("float4") && !trimmed.contains("int4") {
            issues.push(KernelIssue {
                line: i + 1,
                severity: Severity::Info,
                message: "tile_load/tile_store without vector types".into(),
                suggestion: "Use float4/int4 vector types for tile loads to maximize memory bandwidth.".into(),
            });
        }

        // Detect bank conflict potential
        if trimmed.contains("__shared__") && (trimmed.contains("tile") || trimmed.contains("TILE")) {
            if !source.contains("padded") && !source.contains("PADDED") && !source.contains("padding") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Info,
                    message: "Shared memory tile without padding".into(),
                    suggestion: "Pad shared memory tile rows by 1 element (e.g., TILE_DIM + 1) to avoid bank conflicts.".into(),
                });
            }
        }
    }

    issues
}

// ── TileLang Analysis ──

fn analyze_tilelang(source: &str) -> Vec<KernelIssue> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect missing target specification
        if trimmed.contains("tilelang.compile") || trimmed.contains("tilelang.build") {
            if !trimmed.contains("target") && !source.contains("target=") {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: "TileLang compile without target specification".into(),
                    suggestion: "Specify target='cuda -arch=sm_86' or target='rocm' for your GPU architecture.".into(),
                });
            }
        }

        // Detect tile sizes
        let re = regex::Regex::new(r"(\w+_?TILE\w*)\s*[=:]\s*(\d+)").unwrap();
        for cap in re.captures_iter(trimmed) {
            let tile_name = &cap[1];
            let tile_val = cap[2].parse::<u32>().unwrap_or(0);
            if tile_val > 0 && tile_val < 16 {
                issues.push(KernelIssue {
                    line: i + 1,
                    severity: Severity::Warn,
                    message: format!("Small tile: {} = {} (may underutilize GPU)", tile_name, tile_val),
                    suggestion: "Increase tile size to at least 32 for better GPU utilization.".into(),
                });
            }
        }

        // Detect pipeline depth
        if trimmed.contains("pipeline") || trimmed.contains("stage") {
            if source.contains("num_stages") && !source.contains("num_stages=3") && !source.contains("num_stages=4") && !source.contains("num_stages=5") {
                // Custom pipeline depth — could be fine, note it
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cuda() {
        assert_eq!(detect_language("kernel.cu", "__global__ void foo() {}"), GpuLanguage::Cuda);
    }

    #[test]
    fn test_detect_triton() {
        let src = "@triton.jit\ndef kernel(x):\n    tl.store(x, 1)";
        assert_eq!(detect_language("kernel.py", src), GpuLanguage::Triton);
    }

    #[test]
    fn test_detect_numba() {
        let src = "from numba import cuda\n@cuda.jit\ndef kernel():\n    pass";
        assert_eq!(detect_language("kernel.py", src), GpuLanguage::Numba);
    }

    #[test]
    fn test_detect_pytorch() {
        let src = "model.cuda()\ntorch.compile(model)";
        assert_eq!(detect_language("train.py", src), GpuLanguage::PyTorch);
    }

    #[test]
    fn test_detect_cute() {
        let src = "CUTE_DEVICE void kernel() { TiledMMA mma; }";
        assert_eq!(detect_language("gemm.cuh", src), GpuLanguage::Cute);
    }

    #[test]
    fn test_analyze_triton_missing_autotune() {
        let src = "@triton.jit\ndef kernel(x):\n    tl.store(x, 1)";
        let result = analyze("kernel.py", src);
        assert_eq!(result.language, GpuLanguage::Triton);
        assert!(result.issues.iter().any(|i| i.message.contains("Missing @triton.autotune")));
    }

    #[test]
    fn test_analyze_numba_fastmath() {
        let src = "@cuda.jit\ndef kernel(x):\n    i = cuda.grid(1)\n    x[i] *= 2";
        let result = analyze("kernel.py", src);
        assert_eq!(result.language, GpuLanguage::Numba);
        assert!(result.issues.iter().any(|i| i.message.contains("fastmath")));
    }

    #[test]
    fn test_analyze_pytorch_compile() {
        let src = "model = Model()\nmodel.cuda()\nfor epoch in range(100):\n    out = model(x)";
        let result = analyze("train.py", src);
        assert_eq!(result.language, GpuLanguage::PyTorch);
        assert!(result.issues.iter().any(|i| i.message.contains("torch.compile")));
    }

    #[test]
    fn test_analyze_cute_arch() {
        let src = "CUTE_DEVICE void kernel() { TiledMMA mma; }";
        let result = analyze("gemm.cuh", src);
        assert_eq!(result.language, GpuLanguage::Cute);
        assert!(result.issues.iter().any(|i| i.message.contains("CUTE architecture macro")));
    }
}
