# Multi-Model AI Gateway architecture

Multi-Model AI Gateway is a loopback-only Rust gateway with a browser UI and three inference backends:

```text
Browser / CLI / OpenAI client
            |
            v
   Rust gateway :8788
     |       |       |
     v       v       v
   MLX     Apple   NVIDIA
  :8080   Swift    NIM API
            |
            +-- Private Cloud Compute when available
            +-- on-device Foundation Models fallback
```

## Routing

Requests can select `local`, `apple`, `nvidia`, or `auto` through `extra_body.provider` or the `x-ai-provider` header.

`auto` scores request difficulty from 0–10 and considers in-flight load. If `MULTI_MODEL_GATEWAY_ROUTER_MODEL` points to a compatible trained router JSON file, its provider prediction is used as an additional learned signal; the heuristic remains the fallback.

- difficulty `<= AI_LOCAL_MAX_DIFFICULTY` (default 3): prefer local MLX when it is not materially busier;
- harder requests: prefer NVIDIA;
- agent/tool requests: prefer NVIDIA as the planner, while tools execute locally;
- backend failure: retry on the configured alternate backend;
- Apple is explicit-select or an available alternate, and its bridge falls back from Private Cloud Compute to on-device inference.

NVIDIA model selection is difficulty-aware:

| Difficulty | Default model |
| --- | --- |
| 0–2 | `nvidia/nemotron-3.5-bolt-e4b` |
| 3–6 | `nvidia/nemotron-3.5-lightning-30b-a3b` |
| 7–10 | `deepseek-ai/deepseek-v4.1-flash` |

Only the source allowlist in `src/main.rs` is eligible. `NVIDIA_FREE_MODELS` can narrow that list but cannot add unverified or paid models.

## Local tools

Full-access agent mode is intentional. The gateway can provide local file, Git, shell, and repository context to the model. Commands are bounded by timeout and output limits. The gateway binds to `127.0.0.1` by default; do not expose it to a network interface without adding authentication and a narrower access policy.

## Apple Silicon and clustering

MLX is the primary local inference runtime. EXO/JACCL/RDMA is represented as a cluster integration point for Apple Silicon machines connected over Thunderbolt 5. `EXO_BASE_URL` is currently unset on the reference Mac, so the single-machine MLX path is active. The cluster command reports configuration state but does not pretend that RDMA is active when no cluster endpoint exists.

