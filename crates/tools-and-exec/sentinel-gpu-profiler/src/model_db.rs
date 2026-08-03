use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct ModelInfo {
    pub name: &'static str,
    pub param_count_b: f64,
    pub vram_gb_fp16: f64,
    pub family: &'static str,
    pub recommended_quant: &'static str,
}

pub static MODEL_DB: &[ModelInfo] = &[
    ModelInfo {
        name: "tinyllama",
        param_count_b: 1.1,
        vram_gb_fp16: 0.66,
        family: "llama",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "phi-2",
        param_count_b: 2.7,
        vram_gb_fp16: 1.62,
        family: "phi",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "phi3:mini",
        param_count_b: 3.8,
        vram_gb_fp16: 2.28,
        family: "phi3",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "phi3.5:mini",
        param_count_b: 3.8,
        vram_gb_fp16: 2.28,
        family: "phi3",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "llama3.2:1b",
        param_count_b: 1.0,
        vram_gb_fp16: 0.6,
        family: "llama",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "llama3.2:3b",
        param_count_b: 3.0,
        vram_gb_fp16: 1.8,
        family: "llama",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "llama3.1:8b",
        param_count_b: 8.0,
        vram_gb_fp16: 4.9,
        family: "llama",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "llama3.3:70b",
        param_count_b: 70.0,
        vram_gb_fp16: 42.0,
        family: "llama",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "mistral:7b",
        param_count_b: 7.0,
        vram_gb_fp16: 4.3,
        family: "mistral",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "mixtral:8x7b",
        param_count_b: 47.0,
        vram_gb_fp16: 28.2,
        family: "mixtral",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "qwen2.5:0.5b",
        param_count_b: 0.5,
        vram_gb_fp16: 0.3,
        family: "qwen2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "qwen2.5:1.5b",
        param_count_b: 1.5,
        vram_gb_fp16: 0.9,
        family: "qwen2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "qwen2.5:3b",
        param_count_b: 3.0,
        vram_gb_fp16: 1.8,
        family: "qwen2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "qwen2.5:7b",
        param_count_b: 7.0,
        vram_gb_fp16: 4.3,
        family: "qwen2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "qwen2.5:72b",
        param_count_b: 72.0,
        vram_gb_fp16: 43.2,
        family: "qwen2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "deepseek-r1:8b",
        param_count_b: 8.0,
        vram_gb_fp16: 5.2,
        family: "deepseek",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "deepseek-r1:67b",
        param_count_b: 67.0,
        vram_gb_fp16: 40.2,
        family: "deepseek",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "gemma2:2b",
        param_count_b: 2.0,
        vram_gb_fp16: 1.2,
        family: "gemma2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "gemma2:9b",
        param_count_b: 9.0,
        vram_gb_fp16: 5.4,
        family: "gemma2",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "nemotron-mini:4b",
        param_count_b: 4.0,
        vram_gb_fp16: 2.4,
        family: "nemotron",
        recommended_quant: "Q4_K_M",
    },
    ModelInfo {
        name: "codegemma:2b",
        param_count_b: 2.0,
        vram_gb_fp16: 1.2,
        family: "codegemma",
        recommended_quant: "Q4_K_M",
    },
];

fn build_model_map() -> HashMap<&'static str, &'static ModelInfo> {
    let mut m = HashMap::new();
    for info in MODEL_DB {
        m.insert(info.name, info);
    }
    m
}

pub fn lookup_model(name: &str) -> Option<&'static ModelInfo> {
    let map = build_model_map();
    map.get(name).copied()
}

pub fn filter_models_by_vram(vram_gb: f64, quant_factor: f64) -> Vec<&'static ModelInfo> {
    let factor = if quant_factor > 0.0 {
        quant_factor
    } else {
        2.5
    };
    MODEL_DB
        .iter()
        .filter(|m| m.vram_gb_fp16 * factor <= vram_gb)
        .collect()
}

pub fn estimate_vram(params_b: f64, quant_bits: u32) -> f64 {
    let bytes_per_param = match quant_bits {
        16 => 2.0,
        8 => 1.0,
        4 => 0.5,
        2 => 0.25,
        _ => 2.0,
    };
    let weights_gb = params_b * bytes_per_param;
    let kv_cache_gb = params_b * 0.3;
    let overhead_gb = params_b * 0.1;
    weights_gb + kv_cache_gb + overhead_gb
}

pub static CLOUD_ALTERNATIVES: &[(&str, &str, &str, &str)] = &[
    ("tinyllama", "claude-3-haiku", "Anthropic", "$0.25/M tokens"),
    ("phi-2", "claude-3-haiku", "Anthropic", "$0.25/M tokens"),
    (
        "llama3.2:1b",
        "claude-3-haiku",
        "Anthropic",
        "$0.25/M tokens",
    ),
    (
        "llama3.2:3b",
        "claude-3-haiku",
        "Anthropic",
        "$0.25/M tokens",
    ),
    (
        "llama3.1:8b",
        "claude-3-haiku",
        "Anthropic",
        "$0.25/M tokens",
    ),
    (
        "llama3.3:70b",
        "claude-3-opus",
        "Anthropic",
        "$15.00/M tokens",
    ),
    (
        "deepseek-r1:8b",
        "claude-3-sonnet",
        "Anthropic",
        "$3.00/M tokens",
    ),
    (
        "deepseek-r1:67b",
        "claude-3-opus",
        "Anthropic",
        "$15.00/M tokens",
    ),
    (
        "qwen2.5:72b",
        "claude-3-opus",
        "Anthropic",
        "$15.00/M tokens",
    ),
];

pub fn find_cloud_alternative(
    local_model: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    CLOUD_ALTERNATIVES
        .iter()
        .find(|(local, _, _, _)| *local == local_model)
        .map(|(_, cloud, provider, price)| (*cloud, *provider, *price))
}

pub fn recommend_tier(vram_gb: Option<f64>, mem_gb: f64, has_gpu: bool) -> &'static str {
    if let Some(vram) = vram_gb {
        if vram >= 40.0 {
            return "llama3.3:70b";
        }
        if vram >= 16.0 {
            return "llama3.1:8b";
        }
        if vram >= 6.0 {
            return "llama3.2:3b";
        }
        if vram >= 3.0 {
            return "llama3.2:1b";
        }
    }
    if has_gpu && mem_gb >= 32.0 {
        return "llama3.1:8b";
    }
    if has_gpu && mem_gb >= 16.0 {
        return "llama3.2:3b";
    }
    if mem_gb >= 8.0 {
        return "llama3.2:1b";
    }
    "tinyllama"
}
