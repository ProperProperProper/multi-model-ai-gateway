# Thomas AI security guard

Thomas AI uses a native Rust behavioral guard inspired by KV-first security. The current MLX server does not expose raw transformer KV pages, and hosted NVIDIA/Apple services cannot expose their internal tensors, so the deployed guard uses privacy-preserving proxy signals today:

- prompt injection and jailbreak patterns;
- sensitive-data and destructive-action patterns;
- prompt-length anomalies learned from normal traffic;
- tool-call risk, especially shell and file writes;
- optional aggregate MLX KV telemetry: page count, cache tokens, norm summaries, and attention entropy.

Raw prompts and KV tensors are not persisted by the guard.

## Modes

```text
THOMAS_SECURITY_MODE=observe   # default; record, never block ordinary chat
THOMAS_SECURITY_MODE=guard     # block high-risk agent requests/tool calls
THOMAS_SECURITY_MODE=enforce   # block high-risk requests globally
THOMAS_SECURITY_AGENT_MODE=guard
```

Inspect status at `GET /api/security`. A future MLX runtime adapter can send aggregate telemetry to `POST /api/security/kv`:

```json
{"model":"mlx-community/Qwen3-0.6B-4bit","pages":32,"cache_tokens":512,"mean_norm":1.2,"max_norm":4.1,"attention_entropy":0.8}
```

The multi-agent lab records the guard state in `security.json` for every run. It does not bypass the gateway security layer.
