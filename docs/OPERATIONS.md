# Operations runbook

## Service endpoints

| Endpoint | Purpose |
| --- | --- |
| `http://127.0.0.1:8788/` | Multi-Model AI Gateway web UI |
| `/health` | Gateway liveness |
| `/api/status` | Backend health, routing, access mode, and GitHub status |
| `/metrics` | Prometheus-compatible counters and latency gauges |
| `/v1/models` | OpenAI-compatible model inventory |
| `/v1/chat/completions` | OpenAI-compatible chat API |
| `http://127.0.0.1:8080/v1` | MLX upstream server |

## Persistent services

For a local macOS deployment, configure:

- `com.multi-model-ai-gateway.gateway`: `~/Library/LaunchAgents/com.multi-model-ai-gateway.gateway.plist`
- `com.multi-model-ai-gateway.mlx`: an MLX service configured for your machine
- gateway startup: `./target/release/multi-model-gateway serve`
- logs: configure stdout and stderr paths in your service manager

Check or restart them:

```sh
launchctl print gui/$(id -u)/com.multi-model-ai-gateway.gateway
launchctl print gui/$(id -u)/com.multi-model-ai-gateway.mlx
launchctl kickstart -k gui/$(id -u)/com.multi-model-ai-gateway.mlx
launchctl kickstart -k gui/$(id -u)/com.multi-model-ai-gateway.gateway
```

## Key handling

The gateway reads the NVIDIA credential from:

```text
macOS Keychain service: NVIDIA_API_KEY
macOS Keychain account: your macOS username
```

`NVIDIA_API_KEY` is also accepted for development. Never put a key in `.env`, source files, logs, screenshots, or Git. The API status endpoint deliberately reports only `token_exposure: never returned`.

## MLX recovery

If `/v1/models` works but a completion hangs:

1. inspect `logs/gateway-mlx.err` for model-load errors;
2. restart `com.multi-model-ai-gateway.mlx`;
3. wait for `/v1/models` to return the configured model;
4. test `/v1/chat/completions` directly on port 8080;
5. retest the gateway on port 8788.

This recovery is safe because MLX is supervised by launchd. A model-list response alone is not proof that generation is ready; always test a completion after a model reload.

## NVIDIA failures

Check `/api/status` and `/metrics`. NVIDIA health is cached briefly to avoid hammering the upstream API. Requests are limited to the verified free-model allowlist. If NVIDIA is unavailable, explicit NVIDIA requests fail clearly; automatic requests may use local MLX when difficulty and health permit.

## Learned router deployment

Train and validate a router outside the gateway, then set `MULTI_MODEL_GATEWAY_ROUTER_MODEL` in the gateway launch environment. `/api/status` reports `learned_router: true` when the file loaded successfully. Remove the variable to return to the built-in difficulty heuristic. Never deploy a model trained on secrets or private customer data into a shared repository.

## Apple failures

The Swift bridge reports on-device and Private Cloud availability. A Private Cloud session may still fail at runtime because Apple has not provisioned the session assets; the bridge automatically falls back to on-device Foundation Models. This is reported in the response model name as `apple-foundation-model-fallback`.

