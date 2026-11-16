# Multi-Model AI Gateway security guard

Multi-Model AI Gateway uses a native Rust behavioral guard inspired by KV-first security. The current MLX server does not expose raw transformer KV pages, and hosted NVIDIA/Apple services cannot expose their internal tensors, so the deployed guard uses privacy-preserving proxy signals today:

- prompt injection and jailbreak patterns;
- sensitive-data and destructive-action patterns;
- prompt-length anomalies learned from normal traffic;
- tool-call risk, especially shell and file writes;

## Enforced protection layers

The local deployment runs the following boundaries together:

1. SafeAgents is a local proxy in front of the gateway and checks model tool calls before the harness receives them.
2. The Rust gateway runs with `MULTI_MODEL_GATEWAY_SECURITY_MODE=enforce` and
   `MULTI_MODEL_GATEWAY_SECURITY_AGENT_MODE=enforce`. It rejects injection,
   credential, destructive, shell-escape, and trading-action requests.
3. MLX Sentinel runs in `enforce` mode, scans prompts and generated output, and
   reports aggregate KV telemetry back to the gateway.
4. The gateway and MLX services run under macOS sandbox profiles. Credential
   stores and policy/launch-agent locations are denied to the gateway process.
5. Trading actions are denied at the model boundary. Strategies may produce
   research signals, but order placement remains outside the model and must be
   approved by a deterministic risk/execution layer.

These controls reduce impact; they do not make prompt injection mathematically
impossible. Keep the gateway bound to loopback and keep live execution behind a
separate, non-model risk service.
- optional aggregate MLX KV telemetry: page count, cache tokens, norm summaries, and attention entropy.

Raw prompts and KV tensors are not persisted by the guard.

## Modes

```text
MULTI_MODEL_GATEWAY_SECURITY_MODE=observe   # default; record, never block ordinary chat
MULTI_MODEL_GATEWAY_SECURITY_MODE=guard     # block high-risk agent requests/tool calls
MULTI_MODEL_GATEWAY_SECURITY_MODE=enforce   # block high-risk requests globally
MULTI_MODEL_GATEWAY_SECURITY_AGENT_MODE=guard
```

Inspect status at `GET /api/security`. A future MLX runtime adapter can send aggregate telemetry to `POST /api/security/kv`:

```json
{"model":"mlx-community/Qwen3-0.6B-4bit","pages":32,"cache_tokens":512,"mean_norm":1.2,"max_norm":4.1,"attention_entropy":0.8}
```

The multi-agent lab records the guard state in `security.json` for every run. It does not bypass the gateway security layer.
