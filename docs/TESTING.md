# Testing and verification

Run the static checks from the repository root:

```sh
cargo fmt --all -- --check
cargo build --release --offline
cargo clippy --release --offline -- -D warnings
cargo test --release --offline

cd apple-ai-bridge
swift build -c release
./.build/release/thomas-apple-ai --status
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

Verify that the response includes `_thomas_ai.provider` and `_thomas_ai.latency_ms`. Do not print or save request headers containing credentials.

The live verification matrix is:

| Area | Expected result |
| --- | --- |
| Rust formatting/build/lint/tests | pass |
| Swift bridge status and request | pass; cloud may report fallback |
| launchd gateway | running and points at the private release binary |
| MLX direct completion | returns text, not only `/v1/models` |
| NVIDIA direct route | returns a free-model response |
| Apple route | returns Apple response and latency metadata |
| automatic easy route | normally selects local MLX when healthy |
| UI and CLI | serve, render, and execute bounded commands |
| secret scan | no credentials or generated build artifacts tracked |
