# KV-first security in Thomas AI

## What this is

KV-first security is an inference-time security approach based on the transformer key/value cache. The KV cache stores compressed attention state while a model processes a request. A system that can observe aggregate KV behavior can learn what normal inference looks like and flag unusual requests or model states.

This is useful because not every attack is obvious in the visible prompt or response. Prompt injection, jailbreaks, hidden instructions, tool abuse, data exfiltration, and model drift can produce unusual internal or behavioral signals even when the text appears ordinary.

## What Thomas AI implements today

Thomas AI cannot access raw KV tensors from hosted NVIDIA endpoints or Apple Foundation Models. The current MLX-LM server also does not expose raw KV pages through its OpenAI-compatible API. Therefore Thomas AI uses a safe proxy design today:

```text
request ───────────────┐
                        v
                Rust security guard
                ├─ injection patterns
                ├─ jailbreak patterns
                ├─ sensitive-data patterns
                ├─ destructive-action patterns
                ├─ prompt-length baseline
                └─ tool-call risk
                        |
        MLX / Apple / free NVIDIA gateway
                        |
        optional aggregate MLX KV telemetry
                        |
             allow · observe · block
```

The guard does not persist raw prompts or KV tensors. It stores only counters and small statistical baselines in process memory.

## Request protection

Every chat request passes through the guard before provider routing. It checks for patterns such as:

- `ignore previous instructions` and similar prompt injection language;
- jailbreak or developer-mode requests;
- API keys, passwords, credentials, and private-key requests;
- destructive actions such as force pushes, repository deletion, or database drops;
- abnormal prompt length compared with previously observed normal traffic.

The guard produces a score, reasons, and an action. In the default `observe` mode, the request is allowed while the event is recorded in memory. This keeps ordinary chat behavior unchanged during rollout.

## Tool protection

Agent tool calls are inspected separately. The guard recognizes high-risk commands including:

- `rm -rf`;
- `git reset --hard`;
- `git push --force`;
- credential lookup commands;
- `curl`-to-shell patterns;
- unsafe permission changes.

File writes and command execution are visible as risk signals. Full Mac access remains available, but a suspicious tool call can be stopped before execution when guard or enforce mode is enabled.

## Optional KV telemetry

When a future MLX runtime adapter can expose aggregate statistics, it can send data to:

```http
POST /api/security/kv
Content-Type: application/json
```

Example:

```json
{
  "model": "mlx-community/Qwen3-0.6B-4bit",
  "pages": 32,
  "cache_tokens": 512,
  "mean_norm": 1.2,
  "max_norm": 4.1,
  "attention_entropy": null
}
```

The current guard learns a baseline for `mean_norm`, flags large norm outliers, and rejects invalid entropy values when supplied. The standalone [MLX Sentinel](https://github.com/ProperProperProper/mlx-sentinel) exposes real `mlx_lm.KVCache` statistics and can forward only aggregate telemetry here. It currently reports `null` for attention entropy because `mlx_lm` exposes KV tensors but not attention probability matrices. This endpoint accepts aggregate telemetry only; it is not a raw tensor upload API.

## Operating modes

```sh
# Safe rollout: observe, never block ordinary requests.
export THOMAS_SECURITY_MODE=observe

# Protect agent actions and high-risk tool calls.
export THOMAS_SECURITY_MODE=guard
export THOMAS_SECURITY_AGENT_MODE=guard

# Block strong injection, jailbreak, destructive, and high-risk signals globally.
export THOMAS_SECURITY_MODE=enforce
```

Inspect the current mode and counters:

```sh
curl http://127.0.0.1:8788/api/security
```

## Multi-agent lab integration

The multi-agent lab uses the same gateway, so planner, workers, reviewer, and reporter requests inherit the guard. Each lab run records the security state in `security.json` alongside:

- `plan.json`;
- independent worker outputs;
- `review.json`;
- `report.md`;
- timing and routing metadata.

This makes suspicious or unusual runs reviewable without allowing a worker to bypass the gateway.

## Limitations

This is not yet true raw-KV anomaly detection. True KV-first detection requires an inference runtime that exposes safe aggregate cache statistics. Hosted NVIDIA and Apple services cannot provide those internals to Thomas AI. The proxy guard is therefore the correct non-breaking first stage: it provides useful protection now and leaves a clean adapter boundary for native MLX instrumentation later.

Security decisions should be evaluated with held-out benign traffic and attack simulations. Do not train a baseline from unreviewed or contaminated traffic, and do not automatically treat model-generated labels as ground truth.
