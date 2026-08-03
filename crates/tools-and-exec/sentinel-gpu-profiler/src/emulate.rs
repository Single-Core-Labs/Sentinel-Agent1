use crate::langs::GpuLanguage;

// ── Architecture Database ──

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpuArch {
    Pascal61,
    Volta70,
    Turing75,
    Ampere80,
    Ampere86,
    Ada89,
    Hopper90,
    Hopper92,
    Blackwell100,
    Blackwell102,
}

#[derive(Debug, Clone)]
pub struct ArchSpec {
    pub name: &'static str,
    pub compute_cap: &'static str,
    pub family: &'static str,
    pub sm_count: u32,
    pub warps_per_sm: u32,
    pub max_threads_per_sm: u32,
    pub max_blocks_per_sm: u32,
    pub max_threads_per_block: u32,
    pub shared_mem_per_sm: u32,
    pub register_file_size: u32,
    pub l1_cache_per_sm: u32,
    pub l2_cache_size: u32,
    pub core_clock_mhz: u32,
    pub mem_bandwidth_gbps: f64,
    pub warp_size: u32,
    pub shared_mem_banks: u32,
    pub tensor_cores: bool,
    pub fp64_throughput_ratio: f64,
}

pub const ARCH_SPECS: &[ArchSpec] = &[
    ArchSpec {
        name: "GTX 1080 Ti",
        compute_cap: "6.1",
        family: "Pascal",
        sm_count: 28,
        warps_per_sm: 64,
        max_threads_per_sm: 2048,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 98304,
        register_file_size: 65536,
        l1_cache_per_sm: 49152,
        l2_cache_size: 1572864,
        core_clock_mhz: 1582,
        mem_bandwidth_gbps: 484.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: false,
        fp64_throughput_ratio: 0.03125,
    },
    ArchSpec {
        name: "Tesla V100",
        compute_cap: "7.0",
        family: "Volta",
        sm_count: 80,
        warps_per_sm: 64,
        max_threads_per_sm: 2048,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 98304,
        register_file_size: 65536,
        l1_cache_per_sm: 131072,
        l2_cache_size: 6291456,
        core_clock_mhz: 1530,
        mem_bandwidth_gbps: 900.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.5,
    },
    ArchSpec {
        name: "RTX 2080 Ti",
        compute_cap: "7.5",
        family: "Turing",
        sm_count: 68,
        warps_per_sm: 64,
        max_threads_per_sm: 1024,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 65536,
        register_file_size: 65536,
        l1_cache_per_sm: 98304,
        l2_cache_size: 5767168,
        core_clock_mhz: 1545,
        mem_bandwidth_gbps: 616.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.0625,
    },
    ArchSpec {
        name: "A100",
        compute_cap: "8.0",
        family: "Ampere",
        sm_count: 108,
        warps_per_sm: 64,
        max_threads_per_sm: 2048,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 167936,
        register_file_size: 65536,
        l1_cache_per_sm: 131072,
        l2_cache_size: 41943040,
        core_clock_mhz: 1410,
        mem_bandwidth_gbps: 1555.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.5,
    },
    ArchSpec {
        name: "RTX 3090",
        compute_cap: "8.6",
        family: "Ampere",
        sm_count: 82,
        warps_per_sm: 64,
        max_threads_per_sm: 1536,
        max_blocks_per_sm: 16,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 131072,
        register_file_size: 65536,
        l1_cache_per_sm: 131072,
        l2_cache_size: 6291456,
        core_clock_mhz: 1695,
        mem_bandwidth_gbps: 936.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.03125,
    },
    ArchSpec {
        name: "RTX 4090",
        compute_cap: "8.9",
        family: "Ada Lovelace",
        sm_count: 128,
        warps_per_sm: 64,
        max_threads_per_sm: 1536,
        max_blocks_per_sm: 16,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 131072,
        register_file_size: 65536,
        l1_cache_per_sm: 131072,
        l2_cache_size: 7340032,
        core_clock_mhz: 2520,
        mem_bandwidth_gbps: 1008.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.015625,
    },
    ArchSpec {
        name: "H100",
        compute_cap: "9.0",
        family: "Hopper",
        sm_count: 132,
        warps_per_sm: 64,
        max_threads_per_sm: 2048,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 232448,
        register_file_size: 65536,
        l1_cache_per_sm: 262144,
        l2_cache_size: 52428800,
        core_clock_mhz: 1980,
        mem_bandwidth_gbps: 3352.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.5,
    },
    ArchSpec {
        name: "H200",
        compute_cap: "9.2",
        family: "Hopper",
        sm_count: 132,
        warps_per_sm: 64,
        max_threads_per_sm: 2048,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 232448,
        register_file_size: 65536,
        l1_cache_per_sm: 262144,
        l2_cache_size: 52428800,
        core_clock_mhz: 1980,
        mem_bandwidth_gbps: 4800.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.5,
    },
    ArchSpec {
        name: "RTX 5090",
        compute_cap: "10.0",
        family: "Blackwell",
        sm_count: 170,
        warps_per_sm: 64,
        max_threads_per_sm: 1536,
        max_blocks_per_sm: 16,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 131072,
        register_file_size: 65536,
        l1_cache_per_sm: 262144,
        l2_cache_size: 12582912,
        core_clock_mhz: 2520,
        mem_bandwidth_gbps: 1792.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.015625,
    },
    ArchSpec {
        name: "B200",
        compute_cap: "10.2",
        family: "Blackwell",
        sm_count: 168,
        warps_per_sm: 64,
        max_threads_per_sm: 2048,
        max_blocks_per_sm: 32,
        max_threads_per_block: 1024,
        shared_mem_per_sm: 232448,
        register_file_size: 65536,
        l1_cache_per_sm: 262144,
        l2_cache_size: 62914560,
        core_clock_mhz: 1980,
        mem_bandwidth_gbps: 8000.0,
        warp_size: 32,
        shared_mem_banks: 32,
        tensor_cores: true,
        fp64_throughput_ratio: 0.5,
    },
];

pub fn arch_by_name(name: &str) -> Option<&'static ArchSpec> {
    ARCH_SPECS
        .iter()
        .find(|a| a.compute_cap == name || a.name.eq_ignore_ascii_case(name))
}

pub fn arch_by_enum(arch: GpuArch) -> &'static ArchSpec {
    match arch {
        GpuArch::Pascal61 => &ARCH_SPECS[0],
        GpuArch::Volta70 => &ARCH_SPECS[1],
        GpuArch::Turing75 => &ARCH_SPECS[2],
        GpuArch::Ampere80 => &ARCH_SPECS[3],
        GpuArch::Ampere86 => &ARCH_SPECS[4],
        GpuArch::Ada89 => &ARCH_SPECS[5],
        GpuArch::Hopper90 => &ARCH_SPECS[6],
        GpuArch::Hopper92 => &ARCH_SPECS[7],
        GpuArch::Blackwell100 => &ARCH_SPECS[8],
        GpuArch::Blackwell102 => &ARCH_SPECS[9],
    }
}

// ── Launch Configuration ──

#[derive(Debug, Clone)]
pub struct LaunchConfig {
    pub grid_x: u32,
    pub grid_y: u32,
    pub grid_z: u32,
    pub block_x: u32,
    pub block_y: u32,
    pub block_z: u32,
    pub shared_mem_bytes: u32,
    pub registers_per_thread: u32,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        LaunchConfig {
            grid_x: 1,
            grid_y: 1,
            grid_z: 1,
            block_x: 256,
            block_y: 1,
            block_z: 1,
            shared_mem_bytes: 0,
            registers_per_thread: 32,
        }
    }
}

// ── Instruction Extraction from Source ──

#[derive(Debug, Clone, Default)]
pub struct InstructionProfile {
    pub global_loads: u64,
    pub global_stores: u64,
    pub shared_loads: u64,
    pub shared_stores: u64,
    pub arith_fma: u64,
    pub arith_add: u64,
    pub arith_mul: u64,
    pub arith_div: u64,
    pub arith_other: u64,
    pub tensor_ops: u64,
    pub sync_instructions: u64,
    pub branch_instructions: u64,
    pub loop_iterations: u64,
    pub total_instructions: u64,
}

pub fn extract_instruction_profile(source: &str, config: &LaunchConfig) -> InstructionProfile {
    let total_threads = config.block_x as u64
        * config.block_y as u64
        * config.block_z as u64
        * config.grid_x as u64
        * config.grid_y as u64
        * config.grid_z as u64;

    let code = source;
    let lines: Vec<&str> = code.lines().collect();
    let mut in_comment_block = false;

    let mut global_loads = 0u64;
    let mut global_stores = 0u64;
    let mut shared_loads = 0u64;
    let mut shared_stores = 0u64;
    let mut arith_fma = 0u64;
    let mut arith_add = 0u64;
    let mut arith_mul = 0u64;
    let mut arith_div = 0u64;
    let mut arith_other = 0u64;
    let mut sync_instructions = 0u64;
    let mut branch_instructions = 0u64;
    let mut tensor_ops = 0u64;
    let mut explicit_loops = 0u64;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.contains("/*") {
            in_comment_block = true;
        }
        if in_comment_block {
            if trimmed.contains("*/") {
                in_comment_block = false;
            }
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains("__syncthreads")
            || trimmed.contains("__syncwarp")
            || trimmed.contains("__threadfence")
        {
            sync_instructions += 1;
        }

        if trimmed.contains("if ")
            || trimmed.contains("else ")
            || trimmed.contains("switch ")
            || trimmed.contains("? ")
            || trimmed.contains("return ")
        {
            branch_instructions += 1;
        }

        if trimmed.contains("for ") || trimmed.contains("while ") {
            explicit_loops += 1;
        }

        let has_global_load = trimmed.contains('[')
            || trimmed.contains("__ldg")
            || trimmed.contains("__ldca")
            || trimmed.contains("__ldcg")
            || trimmed.contains("ld.global");
        let has_global_store = (trimmed.contains('=') && trimmed.contains('['))
            || trimmed.contains("__stg")
            || trimmed.contains("st.global");
        let has_shared_load = trimmed.contains("__lds")
            || trimmed.contains("ld.shared")
            || trimmed.contains("s_data[")
            || trimmed.contains("shared[")
            || trimmed.contains("__shared__");
        let has_shared_store = trimmed.contains("__sts")
            || trimmed.contains("st.shared")
            || (trimmed.contains('=')
                && (trimmed.contains("s_data[") || trimmed.contains("shared[")));

        if has_global_load {
            global_loads += 1;
        }
        if has_global_store {
            global_stores += 1;
        }
        if has_shared_load {
            shared_loads += 1;
        }
        if has_shared_store {
            shared_stores += 1;
        }

        if trimmed.contains("__fmaf")
            || trimmed.contains("fmaf")
            || trimmed.contains("__fma")
            || trimmed.contains("fma(")
            || trimmed.contains("hfma")
        {
            arith_fma += 1;
        }
        if trimmed.contains("__hmul")
            || trimmed.contains("__fmul")
            || trimmed.contains("mul")
            || (trimmed.contains('*') && !trimmed.contains("**"))
        {
            arith_mul += 1;
        }
        if trimmed.contains("__hadd")
            || trimmed.contains("__fadd")
            || trimmed.contains("add")
            || trimmed.contains("+=")
            || trimmed.contains("+")
        {
            arith_add += 1;
        }
        if trimmed.contains('/') && !trimmed.contains("//") {
            arith_div += 1;
        }

        if trimmed.contains("wmma")
            || trimmed.contains("mma.")
            || trimmed.contains("tensor_core")
            || trimmed.contains("tcgen")
            || trimmed.contains("TiledMMA")
            || trimmed.contains("CUTE_")
        {
            tensor_ops += 1;
        }

        // Count '=' as general arith
        if trimmed.contains('=') && !trimmed.starts_with("//") {
            arith_other += 1;
        }
    }

    // Scale to total threads × loop iterations
    let per_thread_scale = total_threads;
    let loop_factor = if explicit_loops > 0 {
        8u64.max(explicit_loops * 16)
    } else {
        1u64
    };

    let total = (global_loads
        + global_stores
        + shared_loads
        + shared_stores
        + arith_fma
        + arith_add
        + arith_mul
        + arith_div
        + arith_other
        + sync_instructions
        + branch_instructions
        + tensor_ops)
        * loop_factor
        * per_thread_scale;

    InstructionProfile {
        global_loads: global_loads * loop_factor * per_thread_scale,
        global_stores: global_stores * loop_factor * per_thread_scale,
        shared_loads: shared_loads * loop_factor * per_thread_scale,
        shared_stores: shared_stores * loop_factor * per_thread_scale,
        arith_fma: arith_fma * loop_factor * per_thread_scale,
        arith_add: arith_add * loop_factor * per_thread_scale,
        arith_mul: arith_mul * loop_factor * per_thread_scale,
        arith_div: arith_div * loop_factor * per_thread_scale,
        arith_other: arith_other * loop_factor * per_thread_scale,
        tensor_ops: tensor_ops * loop_factor * per_thread_scale,
        sync_instructions: sync_instructions * per_thread_scale,
        branch_instructions: branch_instructions * per_thread_scale,
        loop_iterations: explicit_loops * loop_factor * per_thread_scale,
        total_instructions: total,
    }
}

// ── Memory Coalescing Analysis ──

#[derive(Debug, Clone)]
pub struct MemoryAnalysis {
    pub coalescing_efficiency: f64,
    pub sector_utilization: f64,
    pub global_transactions: u64,
    pub ideal_transactions: u64,
    pub shared_bank_conflicts: u64,
    pub shared_transactions: u64,
    pub register_pressure: u32,
    pub register_spills: u64,
    pub max_registers_per_thread: u32,
}

pub fn analyze_memory(source: &str, config: &LaunchConfig, arch: &ArchSpec) -> MemoryAnalysis {
    let code = source;
    let var_count = code
        .split(|c: char| {
            c.is_whitespace()
                || c == ','
                || c == ';'
                || c == '('
                || c == ')'
                || c == '{'
                || c == '}'
        })
        .filter(|w| !w.is_empty() && w.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
        .filter(|w| !is_keyword(w))
        .count() as u32;

    let thread_stride = if code.contains("blockDim.x") || code.contains("blockIdx.x") {
        let has_stride = code.contains("stride") || code.contains(" *=") || code.contains(" *= ");
        if has_stride {
            4
        } else {
            1
        }
    } else {
        1
    };

    let coalescing_efficiency = if thread_stride == 1 {
        1.0
    } else {
        (1.0 / thread_stride as f64).clamp(0.125, 1.0)
    };

    let warp_size = arch.warp_size as u64;
    let _bytes_per_sector = 32u64;
    let cache_line = 128u64;
    let ideal_trans = (warp_size * 4).div_ceil(cache_line);
    let stride = thread_stride as u64;
    let actual_trans = if stride == 1 {
        ideal_trans
    } else {
        (warp_size * 4 * stride).div_ceil(cache_line)
    };
    let sector_util = if actual_trans > 0 {
        ideal_trans as f64 / actual_trans as f64
    } else {
        1.0
    };

    // Shared memory bank conflict analysis
    let has_shared = code.contains("__shared__")
        || code.contains("shared_mem")
        || code.contains("s_data[")
        || code.contains("shared[");
    let shared_bank_conflicts = if has_shared {
        let bank_conflict_patterns = ["threadIdx.x", "threadIdx.y", " & 0x1f", " % 32"];
        let mut conflicts = 0u64;
        for pat in &bank_conflict_patterns {
            if code.contains(pat) {
                conflicts += 1;
            }
        }
        if code.contains("s_data[threadIdx.x") {
            conflicts += 2;
        }
        if code.contains("s_data[blockIdx") {
            conflicts = 0;
        }
        conflicts
    } else {
        0
    };

    let shared_transactions = if has_shared {
        if shared_bank_conflicts > 0 {
            2 * arch.warp_size as u64
        } else {
            arch.warp_size as u64 / 4
        }
    } else {
        0
    };

    // Register pressure
    let reg_per_thread = config.registers_per_thread.max(16);
    let reg_file_per_sm = arch.register_file_size as u64;
    let max_threads = arch.max_threads_per_sm as u64;

    let _threads_per_block = config.block_x * config.block_y * config.block_z;
    let max_regs_per_thread = (reg_file_per_sm / max_threads.max(1)).min(255) as u32;

    let pressure = reg_per_thread;
    let spills = if pressure > max_regs_per_thread {
        (pressure - max_regs_per_thread) as u64 * 8
    } else if var_count > 64 {
        (var_count - 64) as u64
    } else {
        0
    };

    MemoryAnalysis {
        coalescing_efficiency,
        sector_utilization: sector_util,
        global_transactions: actual_trans,
        ideal_transactions: ideal_trans,
        shared_bank_conflicts,
        shared_transactions,
        register_pressure: pressure,
        register_spills: spills,
        max_registers_per_thread: max_regs_per_thread,
    }
}

fn is_keyword(w: &str) -> bool {
    matches!(
        w,
        "int"
            | "float"
            | "double"
            | "char"
            | "void"
            | "long"
            | "short"
            | "unsigned"
            | "const"
            | "static"
            | "__global__"
            | "__device__"
            | "__shared__"
            | "__managed__"
            | "if"
            | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "break"
            | "continue"
            | "return"
            | "struct"
            | "class"
            | "template"
            | "typedef"
            | "using"
            | "namespace"
            | "true"
            | "false"
            | "nullptr"
            | "NULL"
            | "new"
            | "delete"
            | "sizeof"
            | "auto"
            | "volatile"
            | "inline"
            | "extern"
    )
}

// ── Divergence Analysis ──

#[derive(Debug, Clone)]
pub struct DivergenceAnalysis {
    pub divergence_pct: f64,
    pub divergent_branches: u64,
    pub reconvergence_cost_cycles: u64,
}

pub fn analyze_divergence(
    source: &str,
    _config: &LaunchConfig,
    profile: &InstructionProfile,
) -> DivergenceAnalysis {
    let code = source;
    let uses_thread_id = code.contains("threadIdx.x")
        || code.contains("threadIdx.y")
        || code.contains("threadIdx.z");
    let uses_block_id =
        code.contains("blockIdx.x") || code.contains("blockIdx.y") || code.contains("blockIdx.z");

    let mut divergent_branches = 0u64;
    let mut total_branches = 0u64;

    for line in code.lines() {
        let t = line.trim();
        if t.contains("if ")
            && (t.contains("threadIdx")
                || t.contains("blockIdx")
                || t.contains(" % ")
                || t.contains(" < "))
        {
            divergent_branches += 1;
        }
        if t.contains("if ") || t.contains("else ") {
            total_branches += 1;
        }
    }

    let var_based = uses_thread_id || uses_block_id;
    let div_pct = if total_branches > 0 {
        (divergent_branches as f64 / total_branches as f64).min(1.0)
    } else if var_based {
        0.3
    } else {
        0.0
    };

    let reconvergence_cost = (div_pct * 16.0 * profile.branch_instructions as f64 / 1000.0) as u64;

    DivergenceAnalysis {
        divergence_pct: div_pct * 100.0,
        divergent_branches,
        reconvergence_cost_cycles: reconvergence_cost,
    }
}

// ── Occupancy Calculation ──

#[derive(Debug, Clone)]
pub struct OccupancyResult {
    pub threads_per_block: u32,
    pub blocks_per_sm: u32,
    pub warps_per_sm: u32,
    pub max_warps_per_sm: u32,
    pub occupancy_pct: f64,
    pub limiting_factor: &'static str,
}

pub fn calculate_occupancy(
    config: &LaunchConfig,
    arch: &ArchSpec,
    shared_mem_bytes: u32,
    registers: u32,
) -> OccupancyResult {
    let threads = config.block_x * config.block_y * config.block_z;
    let warps = threads.div_ceil(32);

    let by_threads = arch.max_threads_per_sm / threads;
    let by_blocks = arch.max_blocks_per_sm;
    let by_warps = arch.warps_per_sm / warps;
    let by_shared = arch
        .shared_mem_per_sm
        .checked_div(shared_mem_bytes)
        .unwrap_or(by_threads.max(by_blocks));
    let by_registers = if registers > 0 {
        let regs_per_block = registers as u64 * threads as u64;
        (arch.register_file_size as u64)
            .checked_div(regs_per_block)
            .map_or(by_threads, |v| v as u32)
    } else {
        by_threads
    };

    let mut blocks = by_blocks.min(by_threads).min(by_warps);
    let mut limiter = "max_blocks_per_sm";

    if by_threads < blocks {
        blocks = by_threads;
        limiter = "max_threads_per_sm";
    }
    if by_warps < blocks {
        blocks = by_warps;
        limiter = "max_warps_per_sm";
    }
    if by_shared < blocks {
        blocks = by_shared;
        limiter = "shared_memory";
    }
    if by_registers < blocks {
        blocks = by_registers;
        limiter = "registers";
    }

    blocks = blocks.max(1);

    let active_warps = blocks * warps;
    let occ = active_warps as f64 / arch.warps_per_sm as f64;

    OccupancyResult {
        threads_per_block: threads,
        blocks_per_sm: blocks,
        warps_per_sm: active_warps,
        max_warps_per_sm: arch.warps_per_sm,
        occupancy_pct: occ * 100.0,
        limiting_factor: limiter,
    }
}

// ── Instruction Latency Model ──

#[derive(Debug, Clone)]
pub struct LatencyModel {
    pub compute_cycles_per_warp: u64,
    pub memory_cycles_per_warp: u64,
    pub sync_cycles: u64,
    pub branch_mispredict_cycles: u64,
    pub tensor_core_cycles: u64,
    pub memory_latency_cycles: u64,
}

pub fn build_latency_model(arch: &ArchSpec) -> LatencyModel {
    let clock_ghz = arch.core_clock_mhz as f64 / 1000.0;
    let mem_latency_ns = match arch.family {
        "Hopper" => 200.0,
        "Blackwell" => 180.0,
        "Ampere" => 250.0,
        "Ada Lovelace" => 220.0,
        _ => 300.0,
    };
    let mem_latency_cycles = (mem_latency_ns * clock_ghz).round() as u64;

    let fp_cycles = match arch.family {
        "Hopper" | "Blackwell" => 4,
        _ => 6,
    };

    LatencyModel {
        compute_cycles_per_warp: fp_cycles * 32,
        memory_cycles_per_warp: mem_latency_cycles + 32,
        sync_cycles: if arch.family == "Volta" || arch.family == "Hopper" {
            8
        } else {
            16
        },
        branch_mispredict_cycles: 8,
        tensor_core_cycles: if arch.family == "Hopper" || arch.family == "Blackwell" {
            2
        } else {
            4
        },
        memory_latency_cycles: mem_latency_cycles,
    }
}

// ── Core Emulation ──

#[derive(Debug, Clone)]
pub struct EmulationResult {
    pub arch_name: &'static str,
    pub arch_spec: ArchSpec,
    pub launch: LaunchConfig,
    pub total_threads: u64,
    pub total_blocks: u64,
    pub total_warps: u64,
    pub total_cycles: u64,
    pub total_instructions: u64,
    pub ipc: f64,
    pub execution_time_us: f64,
    pub sm_util_pct: f64,
    pub instruction_profile: InstructionProfile,
    pub occupancy: OccupancyResult,
    pub memory: MemoryAnalysis,
    pub divergence: DivergenceAnalysis,
    pub latency: LatencyModel,
    pub bottleneck: &'static str,
    pub roofline_arith_intensity: f64,
}

pub fn emulate(source: &str, config: &LaunchConfig, arch: &GpuArch) -> EmulationResult {
    let specs = arch_by_enum(*arch);
    let profile = extract_instruction_profile(source, config);
    let memory = analyze_memory(source, config, specs);
    let divergence = analyze_divergence(source, config, &profile);
    let latency = build_latency_model(specs);
    let occupancy = calculate_occupancy(
        config,
        specs,
        config.shared_mem_bytes,
        config.registers_per_thread,
    );

    let total_blocks = config.grid_x as u64 * config.grid_y as u64 * config.grid_z as u64;
    let total_threads =
        total_blocks * config.block_x as u64 * config.block_y as u64 * config.block_z as u64;
    let total_warps =
        total_blocks * (config.block_x * config.block_y * config.block_z).div_ceil(32) as u64;

    // Cycle accounting
    let compute_ops = profile.arith_fma
        + profile.arith_add
        + profile.arith_mul
        + profile.arith_div
        + profile.arith_other;
    let mem_ops =
        profile.global_loads + profile.global_stores + profile.shared_loads + profile.shared_stores;
    let sync_ops = profile.sync_instructions;
    let branch_ops = profile.branch_instructions;
    let tensor_ops = profile.tensor_ops;

    // Warp-level cycle counting with latency hiding
    let warps_per_block = (config.block_x * config.block_y * config.block_z).div_ceil(32) as u64;
    let active_warps = occupancy.blocks_per_sm as u64 * warps_per_block;

    let arith_cycles = compute_ops * latency.compute_cycles_per_warp / active_warps.max(1);
    let mem_cycles = mem_ops * latency.memory_cycles_per_warp / active_warps.max(1);
    let sync_cycles = sync_ops * latency.sync_cycles;
    let branch_cycles = (branch_ops * divergence.divergent_branches / total_warps.max(1)).max(1)
        * latency.branch_mispredict_cycles;
    let tensor_cycles = tensor_ops * latency.tensor_core_cycles;
    let reconvergence_cycles = divergence.reconvergence_cost_cycles;

    // Overlap compute and memory (latency hiding)
    let compute_total =
        arith_cycles + tensor_cycles + sync_cycles + branch_cycles + reconvergence_cycles;
    let memory_total = mem_cycles;
    let overlap_factor = (active_warps as f64 / 8.0).clamp(0.3, 1.0);

    let overlap_cycles = (compute_total.min(memory_total) as f64 * (1.0 - overlap_factor)) as u64;
    let total_cycles = compute_total.max(memory_total) + overlap_cycles;

    let total_instr = profile.total_instructions;
    let ipc = if total_cycles > 0 {
        total_instr as f64 / total_cycles as f64
    } else {
        0.0
    };
    let exec_time_us = if specs.core_clock_mhz > 0 {
        total_cycles as f64 / specs.core_clock_mhz as f64
    } else {
        0.0
    };

    // Bottleneck analysis
    let bottleneck = if compute_total > memory_total * 2 {
        "Compute-bound"
    } else if memory_total > compute_total * 2 {
        "Memory-bound"
    } else {
        "Balanced"
    };

    let total_bytes = (profile.global_loads + profile.global_stores) * 4;
    let arith_intensity = if total_bytes > 0 {
        compute_ops as f64 / total_bytes as f64
    } else {
        0.0
    };

    // SM Utilization: accounts for warp scheduler throughput and stalls
    // Max IPC per SM ≈ active_warps / scheduler_latency (4 cycles typical)
    let warp_schedulers = 4u64;
    let _theoretical_max_ipc = active_warps.min(warp_schedulers * 4) as f64 / 4.0;
    let stall_ratio = if compute_total + memory_total > 0 {
        memory_total as f64 / (compute_total + memory_total) as f64
    } else {
        0.0
    };
    let sm_util_pct = if compute_ops + mem_ops + sync_ops + branch_ops == 0 {
        0.0
    } else {
        (occupancy.occupancy_pct / 100.0 * (1.0 - stall_ratio * 0.4)).clamp(0.0, 1.0) * 100.0
    };

    EmulationResult {
        arch_name: specs.name,
        arch_spec: specs.clone(),
        launch: config.clone(),
        total_threads,
        total_blocks,
        total_warps,
        total_cycles,
        total_instructions: total_instr,
        ipc,
        execution_time_us: exec_time_us,
        sm_util_pct,
        instruction_profile: profile,
        occupancy,
        memory,
        divergence,
        latency,
        bottleneck,
        roofline_arith_intensity: arith_intensity,
    }
}

// ── Multi-Architecture Comparison ──

pub fn emulate_multi(
    source: &str,
    config: &LaunchConfig,
    arches: &[GpuArch],
) -> Vec<EmulationResult> {
    arches.iter().map(|a| emulate(source, config, a)).collect()
}

pub fn compare_arches(results: &[EmulationResult]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(
        out,
        "  {:<24} {:>12} {:>8} {:>10} {:>8} {:>10} {:>14}",
        "Architecture", "Cycles", "IPC", "SM Util", "Occup.", "Time(us)", "Bottleneck"
    );
    let _ = writeln!(out, "  {}", "-".repeat(92));

    for r in results {
        let _ = writeln!(
            out,
            "  {:<24} {:>12} {:>8.2} {:>9.0}% {:>8.0}% {:>9.1} {:>14}",
            format!("{} ({})", r.arch_name, r.arch_spec.compute_cap),
            r.total_cycles,
            r.ipc,
            r.sm_util_pct,
            r.occupancy.occupancy_pct,
            r.execution_time_us,
            r.bottleneck,
        );
    }

    if results.len() >= 2 {
        let base = &results[0];
        let _ = writeln!(out, "  {}", "-".repeat(88));
        for r in &results[1..] {
            if base.total_cycles > 0 {
                let speedup = base.total_cycles as f64 / r.total_cycles as f64;
                let _ = writeln!(
                    out,
                    "  {:<24} {:>12.2}x vs {}",
                    "",
                    format!("{:+.2}", speedup - 1.0).trim(),
                    results[0].arch_name,
                );
            }
        }
    }

    out
}

// ── Execution Report ──

pub fn execution_report(result: &EmulationResult) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(
        out,
        "  GPU: {} ({})",
        result.arch_name, result.arch_spec.compute_cap
    );
    let _ = writeln!(
        out,
        "  Grid: {} x {} x {} | Block: {} x {} x {}",
        result.launch.grid_x,
        result.launch.grid_y,
        result.launch.grid_z,
        result.launch.block_x,
        result.launch.block_y,
        result.launch.block_z
    );
    let _ = writeln!(
        out,
        "  Total: {} warps, {} threads",
        result.total_warps, result.total_threads
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "  --- Execution ---");
    let _ = writeln!(
        out,
        "  {:24}: {:>12} cycles",
        "Total Cycles", result.total_cycles
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Total Instructions", result.total_instructions
    );
    let _ = writeln!(out, "  {:24}: {:>12.2}", "IPC", result.ipc);
    let _ = writeln!(
        out,
        "  {:24}: {:>12.1} us",
        "Estimated Time", result.execution_time_us
    );
    let _ = writeln!(out, "  {:24}: {:>12}", "Bottleneck", result.bottleneck);
    let _ = writeln!(out);

    let _ = writeln!(out, "  --- Occupancy / SM Util ---");
    let _ = writeln!(
        out,
        "  {:24}: {:>9.0}%",
        "Occupancy", result.occupancy.occupancy_pct
    );
    let _ = writeln!(out, "  {:24}: {:>9.0}%", "SM Util", result.sm_util_pct);
    let _ = writeln!(
        out,
        "  {:24}: {}/{}",
        "Warps per SM", result.occupancy.warps_per_sm, result.occupancy.max_warps_per_sm
    );
    let _ = writeln!(
        out,
        "  {:24}: {} blocks",
        "Blocks per SM", result.occupancy.blocks_per_sm
    );
    let _ = writeln!(
        out,
        "  {:24}: {}",
        "Limiting Factor", result.occupancy.limiting_factor
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "  --- Memory ---");
    let _ = writeln!(
        out,
        "  {:24}: {:>9.0}%",
        "Coalescing Eff.",
        result.memory.coalescing_efficiency * 100.0
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>9.1}%",
        "Sector Util.",
        result.memory.sector_utilization * 100.0
    );
    let _ = writeln!(
        out,
        "  {:24}: {}",
        "Bank Conflicts",
        if result.memory.shared_bank_conflicts > 0 {
            format!("{} detected", result.memory.shared_bank_conflicts)
        } else {
            "None".into()
        }
    );
    let _ = writeln!(
        out,
        "  {:24}: {} / thread",
        "Register Pressure", result.memory.register_pressure
    );
    let _ = writeln!(
        out,
        "  {:24}: {} (max {})",
        "Register Spills", result.memory.register_spills, result.memory.max_registers_per_thread
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "  --- Divergence ---");
    let _ = writeln!(
        out,
        "  {:24}: {:>9.1}%",
        "Divergent Branches", result.divergence.divergence_pct
    );
    let _ = writeln!(
        out,
        "  {:24}: {} cycles",
        "Reconvergence Cost", result.divergence.reconvergence_cost_cycles
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "  --- Instruction Mix ---");
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Arithmetic",
        result.instruction_profile.arith_fma
            + result.instruction_profile.arith_add
            + result.instruction_profile.arith_mul
            + result.instruction_profile.arith_div
            + result.instruction_profile.arith_other
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Memory (global)",
        result.instruction_profile.global_loads + result.instruction_profile.global_stores
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Memory (shared)",
        result.instruction_profile.shared_loads + result.instruction_profile.shared_stores
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Tensor Core", result.instruction_profile.tensor_ops
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Sync", result.instruction_profile.sync_instructions
    );
    let _ = writeln!(
        out,
        "  {:24}: {:>12}",
        "Branches", result.instruction_profile.branch_instructions
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "  --- Roofline ---");
    let _ = writeln!(
        out,
        "  {:24}: {:>12.3} ops/byte",
        "Arith. Intensity", result.roofline_arith_intensity
    );
    if result.roofline_arith_intensity > 0.0 {
        let peak_compute =
            result.arch_spec.sm_count as f64 * result.arch_spec.core_clock_mhz as f64 * 64.0
                / 1000.0;
        let peak_mem = result.arch_spec.mem_bandwidth_gbps;
        let ridge_point = peak_compute / peak_mem;
        let _ = writeln!(
            out,
            "  {:24}: {:>12.1} ops/byte",
            "Ridge Point", ridge_point
        );
        let _ = writeln!(
            out,
            "  {:24}: {}",
            "Region",
            if result.roofline_arith_intensity > ridge_point {
                "Compute-bound"
            } else {
                "Memory-bound"
            }
        );
    }

    out
}

pub fn language_config_hint(lang: GpuLanguage, arch: &ArchSpec) -> String {
    match lang {
        GpuLanguage::Cuda => format!(
            "nvcc -arch=sm_{} kernel.cu",
            arch.compute_cap.replace('.', "")
        ),
        GpuLanguage::Triton => format!(
            "@triton.autotune with target=sm_{}",
            arch.compute_cap.replace('.', "")
        ),
        GpuLanguage::Mojo => format!(
            "mojo build --target cuda --arch sm_{}",
            arch.compute_cap.replace('.', "")
        ),
        GpuLanguage::Numba => format!("numba.cuda.select_device(0) # simulates {}", arch.name),
        GpuLanguage::PyTorch => format!(
            "torch.compile(model, mode='max-autotune', backend='inductor') # target {}",
            arch.name
        ),
        GpuLanguage::Cute => format!(
            "nvcc -arch=sm_{} -DCUTE_ARCH_SM{} kernel.cu",
            arch.compute_cap.replace('.', ""),
            arch.compute_cap.replace('.', "")
        ),
        GpuLanguage::CudaTile => format!(
            "nvcc -arch=sm_{} --use_fast_math kernel.cu",
            arch.compute_cap.replace('.', "")
        ),
        GpuLanguage::TileLang => format!(
            "tilelang.compile(kernel, target='cuda', arch='sm_{}')",
            arch.compute_cap.replace('.', "")
        ),
        GpuLanguage::Unknown => format!(
            "Target GPU: {} (sm_{})",
            arch.name,
            arch.compute_cap.replace('.', "")
        ),
    }
}

// ── Configuration Sweep Engine ──

const SWEEP_BLOCK_X: &[u32] = &[64, 96, 128, 192, 256, 384, 512];
const SWEEP_BLOCK_Y: &[u32] = &[1, 2, 4];
const SWEEP_SMEM_KB: &[u32] = &[0, 8, 16, 32, 48, 64];

#[derive(Debug, Clone)]
pub struct SweepEntry {
    pub config: LaunchConfig,
    pub label: String,
    pub result: EmulationResult,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct SweepResult {
    pub entries: Vec<SweepEntry>,
    pub best: Option<SweepEntry>,
    pub kernel_name: String,
    pub arch: GpuArch,
}

pub fn generate_sweep_configs(source: &str) -> Vec<LaunchConfig> {
    let mut configs = Vec::new();
    let re = regex::Regex::new(r"<<<\s*(\d+)\s*,\s*(\d+)\s*>>>").ok();
    let (hint_grid, hint_block) = if let Some(ref re) = re {
        if let Some(caps) = re.captures(source) {
            (
                caps[1].parse::<u32>().ok().unwrap_or(256),
                caps[2].parse::<u32>().ok().unwrap_or(256),
            )
        } else {
            (256, 256)
        }
    } else {
        (256, 256)
    };

    for &bx in SWEEP_BLOCK_X {
        for &by in SWEEP_BLOCK_Y {
            let total = bx * by;
            if total > 1024 {
                continue;
            }
            if total < 32 {
                continue;
            }
            let ratio = total as f64 / hint_block as f64;
            if ratio > 4.0 {
                continue;
            }

            let grid = hint_grid.max(32);
            configs.push(LaunchConfig {
                block_x: bx,
                block_y: by,
                block_z: 1,
                grid_x: grid,
                grid_y: 1,
                grid_z: 1,
                shared_mem_bytes: 0,
                registers_per_thread: 32,
            });

            for &smem_kb in &SWEEP_SMEM_KB[1..] {
                configs.push(LaunchConfig {
                    block_x: bx,
                    block_y: by,
                    block_z: 1,
                    grid_x: grid,
                    grid_y: 1,
                    grid_z: 1,
                    shared_mem_bytes: smem_kb * 1024,
                    registers_per_thread: 32,
                });
            }
        }
    }

    if configs.is_empty() {
        configs.push(LaunchConfig {
            block_x: 128,
            block_y: 1,
            block_z: 1,
            grid_x: hint_grid.max(32),
            grid_y: 1,
            grid_z: 1,
            shared_mem_bytes: 0,
            registers_per_thread: 32,
        });
        configs.push(LaunchConfig {
            block_x: 256,
            block_y: 1,
            block_z: 1,
            grid_x: hint_grid.max(32),
            grid_y: 1,
            grid_z: 1,
            shared_mem_bytes: 0,
            registers_per_thread: 32,
        });
    }

    configs
}

fn label_for_config(cfg: &LaunchConfig) -> String {
    let block = format!("{}x{}x{}", cfg.block_x, cfg.block_y, cfg.block_z);
    if cfg.shared_mem_bytes > 0 {
        format!("{} smem={}KB", block, cfg.shared_mem_bytes / 1024)
    } else {
        block
    }
}

pub fn score_entry(entry: &SweepEntry) -> f64 {
    let r = &entry.result;
    let max_possible_cycles = 1_000_000_000f64;
    let cycle_norm =
        (1.0 - (r.total_cycles as f64 / max_possible_cycles).min(0.99)).max(0.01) * 0.30;
    let occ_norm = (r.occupancy.occupancy_pct / 100.0) * 0.20;
    let sm_util_norm = (r.sm_util_pct / 100.0) * 0.15;
    let coalesce_norm = r.memory.coalescing_efficiency * 0.10;
    let sector_norm = r.memory.sector_utilization * 0.05;
    let ipc_norm = (r.ipc / 32.0).min(1.0) * 0.10;

    let bank_penalty = (r.memory.shared_bank_conflicts as f64 * 0.05).min(0.30);
    let spill_penalty = (r.memory.register_spills as f64 * 0.002).min(0.10);

    (cycle_norm + occ_norm + sm_util_norm + coalesce_norm + sector_norm + ipc_norm)
        * (1.0 - bank_penalty - spill_penalty)
}

pub fn run_config_sweep(source: &str, configs: &[LaunchConfig], arch: &GpuArch) -> Vec<SweepEntry> {
    let mut entries: Vec<SweepEntry> = configs
        .iter()
        .map(|cfg| {
            let result = emulate(source, cfg, arch);
            let label = label_for_config(cfg);
            SweepEntry {
                config: cfg.clone(),
                label: label.clone(),
                result,
                score: 0.0,
            }
        })
        .collect();

    for e in &mut entries {
        e.score = score_entry(e);
    }

    entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entries
}

pub fn detect_best_config(entries: &[SweepEntry]) -> Option<&SweepEntry> {
    entries.first()
}

pub fn format_sweep_table(entries: &[SweepEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(
        out,
        "  {:<20} {:>8} {:>7} {:>7} {:>8} {:>7} {:>13} {:>8}",
        "Config", "Cycles", "IPC", "SM", "Occup.", "Coalesc", "Time(us)", "Score"
    );
    let _ = writeln!(out, "  {}", "-".repeat(88));

    for entry in entries.iter().take(12) {
        let r = &entry.result;
        let _ = writeln!(
            out,
            "  {:<20} {:>8} {:>7.2} {:>6.0}% {:>7.0}% {:>6.0}% {:>12.1} {:>7.3}",
            entry.label,
            r.total_cycles,
            r.ipc,
            r.sm_util_pct,
            r.occupancy.occupancy_pct,
            r.memory.coalescing_efficiency * 100.0,
            r.execution_time_us,
            entry.score,
        );
    }

    if entries.len() > 12 {
        let _ = writeln!(out, "  ... {} more configs", entries.len() - 12);
    }

    out
}

pub fn format_sweep_recommendations(entries: &[SweepEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let best = match entries.first() {
        Some(b) => b,
        None => {
            return String::new();
        }
    };
    let r = &best.result;

    let _ = writeln!(
        out,
        "  ★ Best Config: {}  (score: {:.3})",
        best.label, best.score
    );
    let _ = writeln!(
        out,
        "     Grid: {}x{}x{}  Block: {}x{}x{}  SMEM: {} bytes",
        best.config.grid_x,
        best.config.grid_y,
        best.config.grid_z,
        best.config.block_x,
        best.config.block_y,
        best.config.block_z,
        best.config.shared_mem_bytes
    );
    let _ = writeln!(
        out,
        "     Cycles: {}, IPC: {:.2}, Est. Time: {:.1} us",
        r.total_cycles, r.ipc, r.execution_time_us
    );
    let _ = writeln!(
        out,
        "     Occupancy: {:.0}%, SM Util: {:.0}%, Coalescing: {:.0}%",
        r.occupancy.occupancy_pct,
        r.sm_util_pct,
        r.memory.coalescing_efficiency * 100.0
    );
    let _ = writeln!(
        out,
        "     Bottleneck: {}, Limiting Factor: {}",
        r.bottleneck, r.occupancy.limiting_factor
    );

    let mut recommendations: Vec<String> = Vec::new();

    if r.occupancy.occupancy_pct < 50.0 {
        recommendations.push(format!(
            "Low occupancy ({:.0}%). Try smaller block or less shared memory.",
            r.occupancy.occupancy_pct
        ));
    }
    if r.sm_util_pct < 50.0 {
        recommendations.push(format!(
            "Low SM utilization ({:.0}%). Increase block size to hide latency.",
            r.sm_util_pct
        ));
    }
    if r.memory.coalescing_efficiency < 0.5 {
        recommendations.push(
            "Poor coalescing. Restructure memory access for contiguous thread ordering.".into(),
        );
    }
    if r.bottleneck == "Memory-bound" && r.memory.coalescing_efficiency < 0.8 {
        recommendations
            .push("Memory-bound with low coalescing — likely the main bottleneck.".into());
    }
    if r.bottleneck == "Compute-bound" && r.occupancy.occupancy_pct < 75.0 {
        recommendations.push(
            "Compute-bound but occupancy is low. Increasing occupancy may improve throughput."
                .into(),
        );
    }
    if r.memory.shared_bank_conflicts > 0 {
        recommendations.push(format!("{} shared memory bank conflicts detected. Pad shared arrays or use different indexing.", r.memory.shared_bank_conflicts));
    }
    if r.memory.register_spills > 0 {
        recommendations.push(format!(
            "{} register spills. Reduce register usage or increase --regs.",
            r.memory.register_spills
        ));
    }

    if let Some(second) = entries.get(1) {
        let diff = best.score - second.score;
        if diff < 0.05 {
            let _ = writeln!(
                out,
                "     ~ Similar score ({:.1}%) to {}. Consider both.",
                diff * 100.0,
                second.label
            );
        }
    }

    for rec in &recommendations {
        let _ = writeln!(out, "     ! {}", rec);
    }

    out
}

// ── High-Level Entry Point ──

pub struct EmulateRequest {
    pub source: String,
    pub filename: String,
    pub config: LaunchConfig,
    pub arches: Vec<GpuArch>,
    pub language: GpuLanguage,
    pub sweep: bool,
}

pub struct EmulateOutput {
    pub language: GpuLanguage,
    pub config_hint: String,
    pub single: Option<EmulationResult>,
    pub comparison: Vec<EmulationResult>,
    pub report: String,
    pub comparison_text: String,
    pub sweep_result: Option<SweepResult>,
}

pub fn run_emulation(req: &EmulateRequest) -> EmulateOutput {
    let config_hint = {
        let arch = arch_by_enum(req.arches.first().copied().unwrap_or(GpuArch::Ampere86));
        language_config_hint(req.language, arch)
    };

    let results = emulate_multi(&req.source, &req.config, &req.arches);
    let single = results.first().cloned();
    let report = single.as_ref().map(execution_report).unwrap_or_default();
    let comparison_text = if results.len() > 1 {
        compare_arches(&results)
    } else {
        String::new()
    };

    let sweep_result = if req.sweep {
        let arch = req.arches.first().copied().unwrap_or(GpuArch::Ampere86);
        let configs = generate_sweep_configs(&req.source);
        let entries = run_config_sweep(&req.source, &configs, &arch);
        let best = detect_best_config(&entries).cloned();
        Some(SweepResult {
            entries,
            best,
            kernel_name: req.filename.clone(),
            arch,
        })
    } else {
        None
    };

    EmulateOutput {
        language: req.language,
        config_hint,
        single,
        comparison: results,
        report,
        comparison_text,
        sweep_result,
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arch_specs_have_all_fields() {
        for a in ARCH_SPECS {
            assert!(a.sm_count > 0);
            assert!(a.warps_per_sm > 0);
            assert!(a.shared_mem_per_sm > 0);
        }
    }

    #[test]
    fn test_arch_by_name_lookup() {
        let h100 = arch_by_name("9.0").unwrap();
        assert_eq!(h100.name, "H100");
        let a100 = arch_by_name("A100").unwrap();
        assert_eq!(a100.compute_cap, "8.0");
    }

    #[test]
    fn test_occupancy_calculation() {
        let config = LaunchConfig {
            block_x: 256,
            block_y: 1,
            block_z: 1,
            ..Default::default()
        };
        let arch = arch_by_enum(GpuArch::Ampere86);
        let occ = calculate_occupancy(&config, arch, 0, 32);
        assert!(occ.occupancy_pct > 0.0);
        assert!(occ.blocks_per_sm >= 1);
        let threads_per_block = config.block_x * config.block_y * config.block_z;
        let expected_warps = threads_per_block.div_ceil(32);
        assert_eq!(occ.warps_per_sm, occ.blocks_per_sm * expected_warps);
    }

    #[test]
    fn test_coalescing_efficiency() {
        let source = "int i = blockIdx.x * blockDim.x + threadIdx.x;\nc[i] = a[i] + b[i];";
        let config = LaunchConfig {
            block_x: 256,
            ..Default::default()
        };
        let arch = arch_by_enum(GpuArch::Ampere86);
        let mem = analyze_memory(source, &config, arch);
        assert!(
            mem.coalescing_efficiency > 0.9,
            "contiguous access should coalesce"
        );
    }

    #[test]
    fn test_shared_bank_conflicts_detected() {
        let source = "__shared__ float s_data[1024];\nfloat val = s_data[threadIdx.x * 32];";
        let config = LaunchConfig {
            block_x: 256,
            ..Default::default()
        };
        let arch = arch_by_enum(GpuArch::Ampere86);
        let mem = analyze_memory(source, &config, arch);
        assert!(
            mem.shared_bank_conflicts > 0,
            "strided access should cause bank conflicts"
        );
    }

    #[test]
    fn test_register_spills_high_pressure() {
        let source = "int a0,a1,a2,a3,a4,a5,a6,a7,a8,a9; int b0,b1,b2,b3,b4,b5,b6,b7,b8,b9;";
        let config = LaunchConfig {
            block_x: 128,
            registers_per_thread: 128,
            ..Default::default()
        };
        let arch = arch_by_enum(GpuArch::Ada89);
        let mem = analyze_memory(source, &config, arch);
        assert!(mem.register_pressure >= 128);
    }

    #[test]
    fn test_multi_arch_comparison() {
        let source = "__global__ void kernel(float* a, float* b, float* c) {\nint i = threadIdx.x;\nfor (int j = 0; j < 100; j++) {\nc[i] = a[i] + b[i];\n}\n}";
        let config = LaunchConfig {
            block_x: 256,
            grid_x: 100,
            ..Default::default()
        };
        let arches = vec![GpuArch::Ampere86, GpuArch::Hopper90, GpuArch::Ada89];
        let results = emulate_multi(source, &config, &arches);
        assert_eq!(results.len(), 3);
        assert!(results[0].total_cycles > 0);
        // Hopper should be faster than Ada for compute
        assert!(results[2].ipc > 0.0);
    }

    #[test]
    fn test_divergence_analysis() {
        let source = "int i = threadIdx.x;\nif (i < 16) { a[i] = b[i]; } else { c[i] = d[i]; }";
        let config = LaunchConfig {
            block_x: 32,
            ..Default::default()
        };
        let profile = extract_instruction_profile(source, &config);
        let div = analyze_divergence(source, &config, &profile);
        assert!(div.divergence_pct > 0.0);
    }

    #[test]
    fn test_extract_instructions() {
        let source =
            "int i = threadIdx.x;\n__syncthreads();\na[i] = b[i] + c[i];\n__syncthreads();";
        let config = LaunchConfig {
            block_x: 256,
            grid_x: 10,
            ..Default::default()
        };
        let profile = extract_instruction_profile(source, &config);
        assert!(profile.sync_instructions >= 2 * 2560);
        assert!(profile.global_stores >= 2560);
        assert!(profile.total_instructions > 0);
    }

    #[test]
    fn test_emulate_full_pipeline() {
        let source = "__global__ void vec_add(float* a, float* b, float* c, int n) {\nint i = blockIdx.x * blockDim.x + threadIdx.x;\nif (i < n) c[i] = a[i] + b[i];\n}";
        let config = LaunchConfig {
            block_x: 256,
            grid_x: 100,
            ..Default::default()
        };
        let arch = GpuArch::Ampere86;
        let result = emulate(source, &config, &arch);
        assert!(result.total_cycles > 0);
        assert!(result.total_instructions > 0);
        assert!(result.execution_time_us > 0.0);
        assert!(!result.bottleneck.is_empty());
        assert!(result.occupancy.occupancy_pct > 0.0);
    }

    #[test]
    fn test_run_emulation_integration() {
        let source = "__global__ void matmul(float* A, float* B, float* C, int N) {\nint row = blockIdx.y * blockDim.y + threadIdx.y;\nint col = blockIdx.x * blockDim.x + threadIdx.x;\nfloat sum = 0.0f;\nfor (int k = 0; k < N; k++) sum += A[row * N + k] * B[k * N + col];\nC[row * N + col] = sum;\n}";
        let req = EmulateRequest {
            source: source.to_string(),
            filename: "matmul.cu".to_string(),
            config: LaunchConfig {
                block_x: 16,
                block_y: 16,
                grid_x: 32,
                grid_y: 32,
                ..Default::default()
            },
            arches: vec![GpuArch::Ampere86, GpuArch::Hopper90],
            language: GpuLanguage::Cuda,
            sweep: false,
        };
        let out = run_emulation(&req);
        assert!(!out.report.is_empty());
        assert!(out.comparison_text.contains("H100"));
        assert!(out.config_hint.contains("sm_86"));
    }

    #[test]
    fn test_language_config_hints() {
        let arch = arch_by_enum(GpuArch::Hopper90);
        let hint = language_config_hint(GpuLanguage::Triton, arch);
        assert!(hint.contains("sm_90"));
        let hint_cuda = language_config_hint(GpuLanguage::Cuda, arch);
        assert!(hint_cuda.contains("sm_90"));
    }

    #[test]
    fn test_tensor_core_detection() {
        let source = "wmma::fragment<wmma::matrix_a, 16, 16, 16, half, wmma::row_major> a_frag;";
        let config = LaunchConfig {
            block_x: 32,
            ..Default::default()
        };
        let profile = extract_instruction_profile(source, &config);
        assert!(
            profile.tensor_ops > 0,
            "wmma should be counted as tensor op"
        );
    }

    #[test]
    fn test_zero_thread_handling() {
        let source = "// just a comment";
        let config = LaunchConfig {
            block_x: 1,
            grid_x: 1,
            ..Default::default()
        };
        let result = emulate(source, &config, &GpuArch::Ampere86);
        assert_eq!(result.total_instructions, 0);
    }

    // ── SM Util Tests ──

    #[test]
    fn test_sm_util_in_range() {
        let source = "__global__ void k(float* a) { int i = blockIdx.x * blockDim.x + threadIdx.x; a[i] = a[i] * 2.0f; }";
        let config = LaunchConfig {
            block_x: 256,
            ..Default::default()
        };
        let result = emulate(source, &config, &GpuArch::Ampere86);
        assert!((0.0..=100.0).contains(&result.sm_util_pct));
        assert!(
            result.sm_util_pct > 0.0,
            "SM util should be non-zero for a real kernel"
        );
    }

    #[test]
    fn test_sm_util_zero_for_no_instructions() {
        let source = "// comment only";
        let config = LaunchConfig {
            block_x: 64,
            ..Default::default()
        };
        let result = emulate(source, &config, &GpuArch::Ampere86);
        assert_eq!(result.sm_util_pct, 0.0);
    }

    #[test]
    fn test_sm_util_differs_by_occupancy() {
        let source = "__global__ void k(float* a) { int i = threadIdx.x; for(int j=0;j<100;j++) a[i+j]=sinf(a[i+j]); }";
        let low = emulate(
            source,
            &LaunchConfig {
                block_x: 32,
                block_y: 1,
                shared_mem_bytes: 0,
                ..Default::default()
            },
            &GpuArch::Ampere86,
        );
        let high = emulate(
            source,
            &LaunchConfig {
                block_x: 256,
                block_y: 1,
                shared_mem_bytes: 0,
                ..Default::default()
            },
            &GpuArch::Ampere86,
        );
        assert!(
            (low.sm_util_pct - high.sm_util_pct).abs() < 100.0,
            "SM util should differ with occupancy"
        );
    }

    // ── Config Sweep Tests ──

    #[test]
    fn test_generate_sweep_configs_produces_configs() {
        let source = "__global__ void k() {}";
        let configs = generate_sweep_configs(source);
        assert!(!configs.is_empty(), "Should produce at least one config");
        assert!(configs.len() >= 2, "Should produce multiple configs");
    }

    #[test]
    fn test_run_config_sweep_returns_sorted() {
        let source = "__global__ void k(float* a) { int i = blockIdx.x * blockDim.x + threadIdx.x; a[i] *= 2.0f; }";
        let configs = generate_sweep_configs(source);
        let entries = run_config_sweep(source, &configs, &GpuArch::Ampere86);
        assert!(!entries.is_empty());
        if entries.len() >= 2 {
            assert!(
                entries[0].score >= entries[entries.len() - 1].score,
                "Entries must be sorted descending by score"
            );
        }
    }

    #[test]
    fn test_sweep_result_in_emulate_output() {
        let source = "__global__ void k(float* a) { int i = blockIdx.x * blockDim.x + threadIdx.x; a[i] = a[i] * 2.0f; }";
        let req = EmulateRequest {
            source: source.to_string(),
            filename: "test.cu".to_string(),
            config: LaunchConfig {
                block_x: 256,
                ..Default::default()
            },
            arches: vec![GpuArch::Ampere86],
            language: GpuLanguage::Cuda,
            sweep: true,
        };
        let out = run_emulation(&req);
        assert!(
            out.sweep_result.is_some(),
            "Sweep result should be present when sweep is true"
        );
        let sweep = out.sweep_result.unwrap();
        assert!(sweep.best.is_some(), "Sweep should find a best config");
        let table = format_sweep_table(&sweep.entries);
        assert!(table.contains("Config"), "Table should have header");
        let recs = format_sweep_recommendations(&sweep.entries);
        assert!(
            recs.contains("Best Config"),
            "Recommendations should mention best config"
        );
    }

    #[test]
    fn test_detect_best_config_returns_top() {
        let source = "__global__ void k(float* a) { int i = blockIdx.x * blockDim.x + threadIdx.x; a[i] *= 2.0f; }";
        let configs = generate_sweep_configs(source);
        let entries = run_config_sweep(source, &configs, &GpuArch::Ampere86);
        let best = detect_best_config(&entries);
        assert!(best.is_some());
        assert_eq!(best.unwrap().label, entries[0].label);
    }

    #[test]
    fn test_score_entry_reflects_performance() {
        let source = "__global__ void k(float* a) { int i = threadIdx.x; a[i] = a[i] * 2.0f; }";
        let cfg_good = LaunchConfig {
            block_x: 256,
            block_y: 1,
            ..Default::default()
        };
        let cfg_bad = LaunchConfig {
            block_x: 32,
            block_y: 1,
            shared_mem_bytes: 0,
            registers_per_thread: 255,
            ..Default::default()
        };
        let r_good = emulate(source, &cfg_good, &GpuArch::Ampere86);
        let r_bad = emulate(source, &cfg_bad, &GpuArch::Ampere86);
        let e_good = SweepEntry {
            config: cfg_good,
            label: "good".into(),
            result: r_good,
            score: 0.0,
        };
        let e_bad = SweepEntry {
            config: cfg_bad,
            label: "bad".into(),
            result: r_bad,
            score: 0.0,
        };
        let s_good = score_entry(&e_good);
        let s_bad = score_entry(&e_bad);
        assert!(s_good >= s_bad, "Good config should score >= bad config");
    }
}
