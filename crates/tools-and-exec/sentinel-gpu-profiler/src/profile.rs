use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ProfileSnapshot {
    pub time_s: f64,
    pub gpu_util: f64,
    pub mem_util: f64,
    pub enc_util: f64,
    pub dec_util: f64,
    pub pcie_tx_mb: f64,
    pub pcie_rx_mb: f64,
    pub mem_clock_mhz: f64,
    pub sm_clock_mhz: f64,
    pub temp_c: f64,
    pub power_w: f64,
}

#[derive(Debug, Clone)]
pub struct ProfileResult {
    pub host: String,
    pub duration_s: f64,
    pub snapshots: Vec<ProfileSnapshot>,
    pub anomalies: Vec<Anomaly>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum BottleneckKind {
    ComputeBound,
    MemoryBound,
    PcieBound,
    CpuBound,
    PowerLimited,
    ThermalThrottling,
}

#[derive(Debug, Clone)]
pub struct Anomaly {
    pub kind: BottleneckKind,
    pub severity: ProfileSeverity,
    pub message: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub enum ProfileSeverity {
    Critical,
    Warn,
    Info,
}

pub fn parse_dmon_csv(text: &str) -> Vec<ProfileSnapshot> {
    let mut snapshots = Vec::new();
    let mut header_idx: HashMap<&str, usize> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Detect header line
        if line.starts_with('#') || (line.contains("gpu") && line.contains("pwr")) {
            header_idx.clear();
            let mut col = 0usize;
            for token in line.split_whitespace() {
                let h = token.trim_start_matches('#');
                if !h.is_empty() && h != "Idx" {
                    header_idx.insert(h, col);
                    col += 1;
                }
            }
            continue;
        }

        if header_idx.is_empty() {
            continue;
        }

        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }

        let get_f64 = |key: &str| -> f64 {
            header_idx
                .get(key)
                .and_then(|&i| cols.get(i))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0)
        };

        snapshots.push(ProfileSnapshot {
            time_s: snapshots.len() as f64,
            gpu_util: get_f64("sm") + get_f64("gr"),
            mem_util: get_f64("mem"),
            enc_util: get_f64("enc"),
            dec_util: get_f64("dec"),
            pcie_tx_mb: get_f64("tx") / 1024.0,
            pcie_rx_mb: get_f64("rx") / 1024.0,
            mem_clock_mhz: get_f64("mclk"),
            sm_clock_mhz: get_f64("sclk"),
            temp_c: get_f64("temp"),
            power_w: get_f64("pwr"),
        });
    }

    snapshots
}

pub fn parse_dmon_csv_generic(text: &str) -> Vec<ProfileSnapshot> {
    let mut snapshots = Vec::new();
    let mut skip_header = true;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if skip_header {
            skip_header = false;
            continue;
        }

        // Format: time, gpu%, mem%, enc%, dec%, pcie_tx, pcie_rx (from nvidia-smi dmon)
        // Or: timestamp, gpu_pwr, gpu_temp, gpu_util, gpu_mem_util (from other tools)
        let cols: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if cols.len() < 3 {
            continue;
        }

        let parse = |i: usize| -> f64 { cols.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.0) };

        snapshots.push(ProfileSnapshot {
            time_s: parse(0),
            gpu_util: parse(1),
            mem_util: parse(2),
            ..Default::default()
        });
    }

    snapshots
}

pub fn analyze_profile(snapshots: &[ProfileSnapshot], host: &str) -> ProfileResult {
    let mut anomalies = Vec::new();
    let mut recommendations = Vec::new();

    if snapshots.is_empty() {
        return ProfileResult {
            host: host.to_string(),
            duration_s: 0.0,
            snapshots: vec![],
            anomalies: vec![],
            recommendations: vec!["No profile data captured.".into()],
        };
    }

    let n = snapshots.len() as f64;
    let avg_gpu = snapshots.iter().map(|s| s.gpu_util).sum::<f64>() / n;
    let avg_mem = snapshots.iter().map(|s| s.mem_util).sum::<f64>() / n;
    let avg_pcie_tx = snapshots.iter().map(|s| s.pcie_tx_mb).sum::<f64>() / n;
    let avg_pcie_rx = snapshots.iter().map(|s| s.pcie_rx_mb).sum::<f64>() / n;
    let avg_temp = snapshots.iter().map(|s| s.temp_c).sum::<f64>() / n;

    // Anomaly: low GPU util + low mem util = CPU-bound
    if avg_gpu < 30.0 && avg_mem < 30.0 {
        anomalies.push(Anomaly {
            kind: BottleneckKind::CpuBound,
            severity: ProfileSeverity::Warn,
            message: format!(
                "GPU underutilized: avg {:.0}% util, {:.0}% mem",
                avg_gpu, avg_mem
            ),
            detail: "GPU is mostly idle. Workload is likely CPU-bound or I/O-bound.".into(),
        });
        recommendations.push("Profile CPU usage alongside GPU: add `top -bn1` sampling.".into());
        recommendations
            .push("Increase batch size or parallelize data loading to feed GPU faster.".into());
    }

    // Anomaly: high GPU util + low mem util = compute-bound
    if avg_gpu > 70.0 && avg_mem < 50.0 {
        anomalies.push(Anomaly {
            kind: BottleneckKind::ComputeBound,
            severity: ProfileSeverity::Info,
            message: format!(
                "Compute-bound: {:.0}% GPU util vs {:.0}% mem util",
                avg_gpu, avg_mem
            ),
            detail: "GPU compute units are the bottleneck. Memory bandwidth has headroom.".into(),
        });
        recommendations.push(
            "Optimize kernel arithmetic intensity: fuse element-wise ops, use Tensor Cores.".into(),
        );
        recommendations.push("Increase block size to hide instruction latency.".into());
    }

    // Anomaly: high mem util = memory-bound
    if avg_mem > 70.0 {
        anomalies.push(Anomaly {
            kind: BottleneckKind::MemoryBound,
            severity: ProfileSeverity::Warn,
            message: format!("Memory-bandwidth-bound: {:.0}% mem util", avg_mem),
            detail:
                "Workload is saturating memory bandwidth. Compute units may be stalled on data."
                    .into(),
        });
        recommendations
            .push("Use shared memory to cache frequently accessed global memory data.".into());
        recommendations.push("Enable FP16/INT8 quantization to reduce memory traffic.".into());
        recommendations.push("Optimize memory access patterns for coalesced loads/stores.".into());
    }

    // Anomaly: PCIe transfer > threshold
    if avg_pcie_tx > 500.0 || avg_pcie_rx > 500.0 {
        anomalies.push(Anomaly {
            kind: BottleneckKind::PcieBound,
            severity: ProfileSeverity::Warn,
            message: format!("High PCIe transfer: {:.0} MB/s TX, {:.0} MB/s RX", avg_pcie_tx, avg_pcie_rx),
            detail: "Significant data movement over PCIe. Host-device communication may be the bottleneck.".into(),
        });
        recommendations
            .push("Use async CUDA streams to overlap data transfer with kernel execution.".into());
        recommendations.push("Enable GPUDirect RDMA if available.".into());
        recommendations
            .push("Move preprocessing to GPU: use CUDA graphs to reduce launch overhead.".into());
    }

    // Anomaly: high temperature
    if avg_temp > 85.0 {
        anomalies.push(Anomaly {
            kind: BottleneckKind::ThermalThrottling,
            severity: ProfileSeverity::Critical,
            message: format!("Thermal throttling risk: avg {:.0}°C", avg_temp),
            detail: "GPU temperature is near throttling threshold. Performance may degrade.".into(),
        });
        recommendations
            .push("Improve cooling: check fan speeds, ambient temperature, and airflow.".into());
        recommendations.push(
            "Reduce power limit with `nvidia-smi -pl <watts>` to control temperature.".into(),
        );
        recommendations.push("Undervolt GPU for sustained performance under load.".into());
    }

    // Anomaly: high power draw
    if snapshots.iter().any(|s| s.power_w > 0.0) {
        let max_power = snapshots.iter().map(|s| s.power_w).fold(0.0_f64, f64::max);
        if max_power > 300.0 {
            anomalies.push(Anomaly {
                kind: BottleneckKind::PowerLimited,
                severity: ProfileSeverity::Info,
                message: format!("Peak power draw: {:.0}W", max_power),
                detail: "GPU is drawing significant power. May be power-limited on shared infrastructure.".into(),
            });
            recommendations.push(
                "Monitor power capping: `nvidia-smi -pm 1` to enable persistence mode.".into(),
            );
        }
    }

    let duration_s = snapshots.last().map(|s| s.time_s).unwrap_or(0.0);

    // Default recommendation if no specific anomaly
    if recommendations.is_empty() {
        recommendations
            .push("Workload is well-balanced. No significant bottlenecks detected.".into());
    }

    ProfileResult {
        host: host.to_string(),
        duration_s,
        snapshots: snapshots.to_vec(),
        anomalies,
        recommendations,
    }
}

pub fn profile_summary_text(result: &ProfileResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Profile of {} ({:.0}s):\n",
        result.host, result.duration_s
    ));

    if !result.snapshots.is_empty() {
        let n = result.snapshots.len() as f64;
        let avg_gpu = result.snapshots.iter().map(|s| s.gpu_util).sum::<f64>() / n;
        let avg_mem = result.snapshots.iter().map(|s| s.mem_util).sum::<f64>() / n;
        let avg_temp = result.snapshots.iter().map(|s| s.temp_c).sum::<f64>() / n;
        out.push_str(&format!("  Avg GPU util: {:.0}%\n", avg_gpu));
        out.push_str(&format!("  Avg Mem util: {:.0}%\n", avg_mem));
        out.push_str(&format!("  Avg Temp:     {:.0}°C\n", avg_temp));
    }

    if !result.anomalies.is_empty() {
        out.push_str("\nAnomalies:\n");
        for a in &result.anomalies {
            let label = match a.severity {
                ProfileSeverity::Critical => "CRIT",
                ProfileSeverity::Warn => "WARN",
                ProfileSeverity::Info => "INFO",
            };
            out.push_str(&format!("  [{}] {}\n", label, a.message));
            out.push_str(&format!("       {}\n", a.detail));
        }
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

    fn make_snapshot(gpu: f64, mem: f64, pcie_tx: f64, pcie_rx: f64, temp: f64) -> ProfileSnapshot {
        ProfileSnapshot {
            time_s: 0.0,
            gpu_util: gpu,
            mem_util: mem,
            enc_util: 0.0,
            dec_util: 0.0,
            pcie_tx_mb: pcie_tx,
            pcie_rx_mb: pcie_rx,
            mem_clock_mhz: 0.0,
            sm_clock_mhz: 0.0,
            temp_c: temp,
            power_w: 0.0,
        }
    }

    #[test]
    fn test_compute_bound_detection() {
        let snapshots = vec![make_snapshot(95.0, 30.0, 10.0, 5.0, 60.0)];
        let result = analyze_profile(&snapshots, "local");
        assert!(result
            .anomalies
            .iter()
            .any(|a| matches!(a.kind, BottleneckKind::ComputeBound)));
    }

    #[test]
    fn test_memory_bound_detection() {
        let snapshots = vec![make_snapshot(80.0, 90.0, 10.0, 5.0, 60.0)];
        let result = analyze_profile(&snapshots, "local");
        assert!(result
            .anomalies
            .iter()
            .any(|a| matches!(a.kind, BottleneckKind::MemoryBound)));
    }

    #[test]
    fn test_cpu_bound_detection() {
        let snapshots = vec![make_snapshot(10.0, 10.0, 10.0, 5.0, 40.0)];
        let result = analyze_profile(&snapshots, "local");
        assert!(result
            .anomalies
            .iter()
            .any(|a| matches!(a.kind, BottleneckKind::CpuBound)));
    }

    #[test]
    fn test_pcie_bound_detection() {
        let snapshots = vec![make_snapshot(50.0, 50.0, 600.0, 500.0, 60.0)];
        let result = analyze_profile(&snapshots, "local");
        assert!(result
            .anomalies
            .iter()
            .any(|a| matches!(a.kind, BottleneckKind::PcieBound)));
    }

    #[test]
    fn test_thermal_detection() {
        let snapshots = vec![make_snapshot(80.0, 80.0, 10.0, 5.0, 90.0)];
        let result = analyze_profile(&snapshots, "local");
        assert!(result
            .anomalies
            .iter()
            .any(|a| matches!(a.kind, BottleneckKind::ThermalThrottling)));
    }

    #[test]
    fn test_no_anomaly() {
        let snapshots = vec![make_snapshot(80.0, 60.0, 100.0, 50.0, 65.0)];
        let result = analyze_profile(&snapshots, "local");
        assert!(result
            .recommendations
            .iter()
            .any(|r| r.contains("well-balanced")));
    }

    #[test]
    fn test_parse_dmon_csv() {
        let csv = "\
# gpu  pwr  temp   sm   mem   enc   dec   mclk  sclk    tx    rx
0   80   60   75   50    0    0  5000  1500   800   400
1   82   61   80   55    0    0  5000  1500   750   380
";
        let snapshots = parse_dmon_csv(csv);
        assert_eq!(snapshots.len(), 2);
        assert!((snapshots[0].gpu_util - 75.0).abs() < 0.1);
        assert!((snapshots[0].mem_util - 50.0).abs() < 0.1);
        assert!((snapshots[0].pcie_tx_mb - 800.0 / 1024.0).abs() < 0.1);
    }
}
