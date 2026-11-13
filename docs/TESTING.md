# Testing and verification

Run the static checks from the repository root:

```sh
cargo fmt --all -- --check
cargo build --release --offline
cargo clippy --release --offline -- -D warnings
cargo test --release --offline

cd apple-ai-bridge
swift build -c release
./.build/release/multi-model-apple-ai --status
```

Run live checks after both launchd services are running:

```sh
curl -fsS http://127.0.0.1:8788/health
curl -fsS http://127.0.0.1:8788/api/status
curl -fsS http://127.0.0.1:8080/v1/models
curl -fsS http://127.0.0.1:8788/v1/models
```

Provider-specific request example:

```sh
curl -fsS http://127.0.0.1:8788/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"auto","messages":[{"role":"user","content":"Reply exactly TEST_OK"}],"extra_body":{"provider":"auto","agent":false},"max_tokens":32}'
```

Verify that the response includes `_multi_model_gateway.provider` and `_multi_model_gateway.latency_ms`. Do not print or save request headers containing credentials.

The live verification matrix is:

| Area | Expected result |
| --- | --- |
| Rust formatting/build/lint/tests | pass |
| Swift bridge status and request | pass; cloud may report fallback |
| gateway | running and points at the release binary |
| MLX direct completion | returns text, not only `/v1/models` |
| NVIDIA direct route | returns a free-model response |
| Apple route | returns Apple response and latency metadata |
| automatic easy route | normally selects local MLX when healthy |
| UI and CLI | serve, render, and execute bounded commands |
| secret scan | no credentials or generated build artifacts tracked |

Self-supervised data preparation can be smoke-tested without training a model:

```sh
python3 ai/ml/self_supervised.py corpus /tmp/multi-model-ai-gateway-self-supervised .
test -s /tmp/multi-model-ai-gateway-self-supervised/train.jsonl
python3 ai/ml/contamination.py /tmp/multi-model-ai-gateway-self-supervised/train.jsonl /tmp/multi-model-ai-gateway-self-supervised/valid.jsonl
```

Do not commit generated corpora until they have been reviewed for secrets and proprietary content.

Model-quality checks should include a held-out validation set, train/validation loss curves, provider accuracy, latency, fallback rate, and failure rate. Treat a rising validation loss or widening train/validation gap as an overfitting failure, not a successful training run.

MCP smoke test:

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | python3 ai/mcp_server.py
```

For a real model call, use `gateway_chat` with `provider: "nvidia"` or `provider: "local"`. The MCP server delegates to the gateway and never receives provider credentials.

Multi-agent lab smoke test:

```sh
python3 skills/multi-agent-ai-lab/scripts/run_lab.py \
  --prompt "Compare two safe router improvements" \
  --workers 2 \
  --provider local \
  --dry-run
```

Remove `--dry-run` for a live run. Artifacts are isolated under `lab_runs/` and are ignored by Git.

Security smoke tests:

```sh
curl -fsS http://127.0.0.1:8788/api/security
curl -fsS -X POST http://127.0.0.1:8788/api/security/kv \
  -H 'content-type: application/json' \
  -d '{"model":"mlx","pages":1,"cache_tokens":8,"mean_norm":1.0,"max_norm":2.0,"attention_entropy":0.8}'
```
