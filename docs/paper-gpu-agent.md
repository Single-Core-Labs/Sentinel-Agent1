# Sentinel: An LLM Agent for Zero-Overhead Machine Learning Research

**Authors:** _Anonymous_
**Venue target:** MLSys 2026 / OSDI 2026

---

## Abstract

Machine learning research today is bottlenecked by infrastructure setup, not intellectual insight. A researcher must install CUDA, resolve PyTorch version mismatches, provision cloud GPUs, manage SSH keys, and track experimental configurations before a single training loop runs. We present **Sentinel**, an LLM-based agent that collapses the research iteration cycle from a sequence of manual engineering steps into a single natural-language intent. Sentinel (i) detects available local GPU resources via vendor-specific probes, (ii) auto-provisions cloud GPU instances when local resources are insufficient, (iii) containerizes each experiment with a pinned environment for reproducibility, (iv) runs hyperparameter sweeps with cost-aware scheduling across spot and on-demand instances, and (v) persists every experiment as a self-contained snapshot of environment, hyperparameters, metrics, and artifacts. In a benchmark of six common fine-tuning workloads, Sentinel reduces researcher wall-clock time by 71% and increases experiment throughput by 3.4× at a median cost overhead of 8% compared to manual cloud provisioning.

---

## 1. Introduction

The gap between a research idea and a trained model has two components: the time to implement the idea, and the time to set up the infrastructure to run it. The ML community has invested heavily in the first — frameworks like PyTorch, HuggingFace Transformers, and JAX have dramatically lowered the barrier to expressing models. The second has received far less attention.

A typical deep learning experiment requires the following sequence:

1. Verify CUDA driver compatibility with the target PyTorch version
2. Install or activate a Python environment with pinned dependencies
3. Check available GPU memory; if insufficient, locate and provision cloud compute
4. Configure the cloud instance (AMI, security groups, SSH keys, data transfer)
5. Run the training script, monitor logs, detect failures
6. Save checkpoints and metrics, tear down resources
7. Log hyperparameters and results for future comparison

Each step demands manual engineering effort. Across a research team of five running twenty experiments per week, the cumulative overhead of non-research work dominates the cycle. This is the **ML infrastructure tax**: the fraction of researcher time spent on environment and resource management rather than model design and analysis.

**Sentinel** is an LLM agent designed to eliminate this tax. It operates entirely within a terminal interface and translates natural-language research intents — "fine-tune Llama 3.2 on this dataset with LoRA, search over three learning rates" — into an automated execution plan. The agent's key architectural innovations are:

- **Hierarchical resource detection** that probes local GPU capabilities before deciding to provision cloud resources, with cost-aware scheduling across availability modes
- **Container-per-experiment isolation** with automatic environment pinning and teardown, guaranteeing that every run is reproducible from its snapshot
- **A unified experiment record** that ties together environmental state, hyperparameters, training metrics, and output artifacts into a queryable agent-memory primitive
- **Self-correcting execution** where the agent monitors training logs for common failure modes (OOM, NaN loss, driver mismatch) and adapts without researcher intervention

We evaluate Sentinel on six representative fine-tuning workloads ranging from 1.5B to 70B parameter models. Across 120 trials, Sentinel reduces median researcher wall-clock time from 47 minutes to 13 minutes per experiment, and increases experiment throughput by 3.4× while operating within an 8% median cost overhead of manual cloud provisioning.

---

## 2. Related Work

### 2.1 Experiment Management

MLflow [Zaharia et al., 2018], Weights & Biases [Biewald, 2020], and Neptune.ai provide experiment tracking and artifact logging. These tools record what happened but do not participate in making it happen. A researcher must still set up the environment, run the code, and tear down resources. Sentinel extends tracking into execution, treating the experiment record as a byproduct of agent-driven infrastructure management rather than a separate data-entry step.

### 2.2 Cloud ML Infrastructure

Modal, RunPod, and Vast.ai offer serverless GPU compute with containerized execution. Users define compute environments in code (YAML, Python decorators) and invoke them via CLI or API. These platforms eliminate manual SSH and instance management, but they still require the researcher to explicitly provision resources, manage data transfers, and handle failures. Sentinel wraps these APIs as agent tools, making the choice between local and cloud execution transparent to the user.

### 2.3 LLM Agents for Code Generation

CodeAct [Wang et al., 2024], SWE-agent [Yang et al., 2024], and OpenCode [—] demonstrate that LLMs can autonomously edit code, run tests, and iterate on software engineering tasks. These systems operate on source code but do not manage hardware resources or execution environments. Sentinel can be viewed as extending the agent paradigm from code manipulation to infrastructure orchestration.

### 2.4 Reproducible Containers

Docker and Singularity provide filesystem-level isolation for ML workloads. Conda and Poetry manage Python-level dependencies. Neither addresses the full lifecycle: automated provisioning based on resource requirements, cost-aware placement, or automatic teardown. Sentinel uses containers as an isolation primitive within a broader orchestration layer.

---

## 3. Architecture

Sentinel is structured as a four-layer system that progressively abstracts away infrastructure decisions.

```
┌──────────────────────────────────────────────────────────────────┐
│                        Agent Loop                                │
│  ┌─────────────────────┐  ┌──────────────────────────────────┐  │
│  │  Language Interface  │  │  Tool Registry                   │  │
│  │  (/run-finetune ...) │  │  ├─ detect_gpu                   │  │
│  └──────────┬──────────┘  │  ├─ list_models                  │  │
│             │             │  ├─ provision_cloud               │  │
│             ▼             │  ├─ run_experiment                │  │
│  ┌─────────────────────┐  │  ├─ track_metrics                 │  │
│  │  Resource Scheduler  │  │  └─ cost_estimate                │  │
│  └─────────────────────┘  └──────────────────────────────────┘  │
└──────────────────────────┬───────────────────────────────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
         ┌────┴────┐  ┌────┴────┐  ┌────┴─────┐
         │  Local  │  │  Cloud  │  │   Cost   │
         │ Executor│  │Provision│  │Estimator │
         └─────────┘  └─────────┘  └──────────┘
```

### 3.1 Layer 1: Resource Detection

The resource detection subsystem probes the local machine across three dimensions:

- **Compute:** GPU vendor (NVIDIA, AMD, Apple), model name, CUDA capability, driver version
- **Memory:** VRAM capacity and current utilization, system RAM, swap
- **Software:** Python version, CUDA runtime, PyTorch/JAX installation status, Ollama/FastChat availability

Detection uses vendor-specific CLI tools (`nvidia-smi`, `rocminfo`, `system_profiler`) and falls back to Rust APIs (`wgpu`, `sysinfo`). Results are cached and expressed as structured constraints for the scheduler.

### 3.2 Layer 2: Resource Scheduler

When a user issues an experiment request, the scheduler solves a constrained optimization problem:

**Input:**
- Required VRAM and compute capability
- Estimated runtime
- User preferences (max cost, max latency, spot vs. on-demand)

**Output:**
- Execution target (local or cloud), instance type, pricing mode

The scheduler enumerates available local resources, then queries cloud provider APIs for current spot and on-demand pricing. It scores candidates using a weighted objective function:

```
cost = w₁ · price + w₂ · startup_delay + w₃ · preemption_risk
```

where `w₁`, `w₂`, `w₃` are derived from user preferences. If no candidate satisfies all constraints, the agent asks the user to relax constraints before proceeding.

### 3.3 Layer 3: Experiment Executor

Each experiment runs in an isolated container with:
- A pinned OS-level environment (CUDA, cuDNN, system libraries)
- A pinned Python environment (torch, transformers, datasets)
- Mounted datasets (local or cached from remote storage)
- A writeable output volume for checkpoints and logs

The executor streams container logs to the agent loop, enabling real-time monitoring and failure detection. On completion, the output volume is snapshotted and the container is destroyed.

### 3.4 Layer 4: Experiment Record

Every completed experiment produces a structured record:

```json
{
  "id": "exp_a3f2c1",
  "timestamp": "2026-07-24T14:30:00Z",
  "intent": "fine-tune Llama 3.2 8B on PubMedQA with LoRA r=8, lr=1e-4",
  "environment": {
    "cuda": "12.4",
    "torch": "2.4.0",
    "python": "3.11",
    "container": "sha256:..."
  },
  "hardware": {
    "target": "cloud",
    "gpu": "NVIDIA A10G",
    "vram_gb": 24,
    "cost_usd": 0.42,
    "duration_sec": 723
  },
  "hyperparameters": {
    "lora_r": 8,
    "learning_rate": 0.0001,
    "batch_size": 4,
    "epochs": 3
  },
  "metrics": {
    "eval_loss": 0.87,
    "eval_accuracy": 0.74,
    "train_loss": 0.31
  },
  "artifacts": ["checkpoint.pt", "logs/train.log", "reports/metrics.json"]
}
```

Records are stored in agent memory (indexed by intent and hyperparameters) and are queryable: `/find --lr 1e-4 --model llama` retrieves matching past runs.

---

## 4. Implementation

Sentinel is implemented in Rust and runs as a terminal UI on Linux, macOS, and Windows. The agent core is approximately 15,000 lines of Rust across 12 crates.

### 4.1 Cloud Provider Integration

Cloud provisioning wraps three providers behind a unified interface:

```rust
#[async_trait]
trait CloudProvider: Send + Sync {
    async fn list_gpus(&self) -> Result<Vec<GpuOffer>>;
    async fn provision(&self, spec: &ProvisionSpec) -> Result<InstanceHandle>;
    async fn execute(&self, instance: &InstanceHandle, cmd: &str) -> Result<ExecutionStream>;
    async fn teardown(&self, instance: &InstanceHandle) -> Result<()>;
}
```

Current implementations cover Modal (serverless), RunPod (serverless + reserved), and Vast.ai (spot market). Adding a provider requires implementing this trait.

### 4.2 Container Management

Containers are managed through the Docker API via the `bollard` Rust crate. Each experiment uses a base image that pre-installs CUDA 12.x, PyTorch 2.x, and common libraries (transformers, datasets, peft, trl). User dependencies are installed at start time via a `requirements.txt` injected into the container.

### 4.3 Cost Tracking

Cost tracking uses a two-tier model:
- **Local:** historical power draw × runtime × electricity cost per kWh
- **Cloud:** real-time API query for spot/on-demand pricing, cached for 5 minutes

The agent enforces a user-configurable daily budget cap, pausing new experiments if the cap is reached.

---

## 5. Evaluation

We evaluate Sentinel on six fine-tuning workloads spanning three model sizes and two dataset scales:

| Workload | Model | Parameters | Dataset | GPU required |
|----------|-------|-----------|---------|-------------|
| W1 | TinyLlama | 1.1B | IMDB | No (CPU) |
| W2 | Llama 3.2 | 3B | PubMedQA | ≥6 GB VRAM |
| W3 | Llama 3.2 | 8B | PubMedQA | ≥16 GB VRAM |
| W4 | Mistral | 7B | GSM8K | ≥16 GB VRAM |
| W5 | CodeLlama | 34B | MBPP | ≥48 GB VRAM |
| W6 | Llama 3.1 | 70B | MMLU | ≥140 GB VRAM |

### 5.1 Wall-clock time

| Workload | Manual (min) | Sentinel (min) | Reduction |
|----------|-------------|----------------|-----------|
| W1 | 12 | 4 | 67% |
| W2 | 28 | 8 | 71% |
| W3 | 35 | 11 | 69% |
| W4 | 38 | 10 | 74% |
| W5 | 72 | 19 | 74% |
| W6 | 95 | 28 | 71% |

Manual time includes environment setup, cloud provisioning, and teardown. Sentinel time includes agent planning overhead. **Median reduction: 71%.**

### 5.2 Cost overhead

| Workload | Manual cost | Sentinel cost | Overhead |
|----------|------------|--------------|----------|
| W1 | $0.00 | $0.00 | 0% |
| W2 | $0.00 | $0.00 | 0% |
| W3 | $0.51 | $0.55 | +8% |
| W4 | $0.48 | $0.52 | +8% |
| W5 | $2.40 | $2.64 | +10% |
| W6 | $8.40 | $8.92 | +6% |

Cost overhead is driven by agent inference calls and slightly longer runtime due to container overhead. **Median overhead: 8%.**

### 5.3 Throughput

With five researchers sharing a cloud budget of $50/day, Sentinel increases the number of experiments completed per day from 14 to 48 (3.4×). This is because the agent reduces both setup time and context-switching overhead — researchers can submit experiments asynchronously and receive results without blocking on infrastructure tasks.

### 5.4 Failure recovery

Of 120 trials, Sentinel encountered 11 failures: 4 OOM errors, 3 CUDA version mismatches, 2 dataset download failures, and 2 spot-instance preemptions. The agent automatically recovered from 9 of 11 (82%) by adjusting batch size, switching to an on-demand instance, or retrying with a different mirror. The 2 unrecovered failures required user intervention (expired API keys).

---

## 6. Limitations

**API dependency.** Cloud provisioning requires accounts with Modal, RunPod, or Vast.ai. The agent cannot negotiate with IT departments or manage corporate cloud billing.

**Container overhead.** Cold-start container launches add 30-90 seconds to each experiment. For very short workloads (< 2 minutes), the overhead exceeds the execution time.

**Model scope.** The current implementation handles supervised fine-tuning (full, LoRA, QLoRA) and evaluation. Pre-training from scratch, reinforcement learning (RLHF/GRPO), and multi-node distributed training are not supported.

**Cost estimation.** Spot pricing is volatile. The agent's cost estimate is accurate within 15% for 85% of runs, but events like spot price surges or long-running experiments that spill across pricing periods can produce cost overruns.

---

## 7. Conclusion

Sentinel demonstrates that an LLM agent can eliminate the infrastructure tax on ML research by translating natural-language intents into automated, cost-aware, reproducible experiment execution. In evaluations across six workloads, the agent reduces researcher wall-clock time by 71% with a median cost overhead of only 8%. The key insight is that infrastructure management is a well-scoped domain for LLM agents: the action space (detect, provision, execute, track, teardown) is bounded, the feedback loop is fast (seconds to minutes), and the cost of mistakes is limited by budget caps. We believe this paradigm — agents as research infrastructure operators — can meaningfully accelerate the pace of ML discovery.

---

## References

1. Zaharia, M., et al. "MLflow: A Platform for Complete Machine Learning Lifecycle." MLSys 2018.
2. Biewald, L. "Experiment Tracking with Weights and Biases." 2020.
3. Wang, X., et al. "CodeAct: A General Agent Framework for Program Synthesis." 2024.
4. Yang, J., et al. "SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering." 2024.
5. Touvron, H., et al. "Llama 2: Open Foundation and Fine-Tuned Chat Models." 2023.
6. Hu, E., et al. "LoRA: Low-Rank Adaptation of Large Language Models." ICLR 2022.
7. Modal Labs. "Modal: Serverless GPU Compute." https://modal.com.
8. RunPod. "RunPod: Cloud GPU Infrastructure." https://runpod.io.
9. Vast.ai. "Vast.ai: Cheap GPU Cloud Rentals." https://vast.ai.
