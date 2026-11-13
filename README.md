# Thomas AI: NVIDIA LLMs, MLX, and Apple AI Services

Thomas AI is a native Rust AI gateway and LM Studio-style desktop web interface for Apple Silicon. It combines local MLX inference, free NVIDIA hosted LLM endpoints, Apple Foundation Models, automatic difficulty and load routing, streaming responses, and full-access local Mac tools.

Keywords: Apple Silicon AI, Rust AI gateway, MLX, NVIDIA NIM API, Nemotron, free NVIDIA LLMs, Apple Foundation Models, local LLM, OpenAI-compatible API, EXO, RDMA, tool calling, agentic coding, macOS AI.

Article: [How Thomas AI combines free NVIDIA LLMs, MLX, and Apple AI services](https://properproperproper.github.io/thomas-ai-nvidia-llms-mlx-apple-ai-services/)

## Features

- Native Rust gateway optimized for Apple Silicon.
- MLX-LM local inference with automatic launchd service management.
- NVIDIA hosted routing restricted to catalog-verified free endpoints.
- Nemotron as the default NVIDIA model, with difficulty-aware model selection.
- Load-aware routing using current local and NVIDIA in-flight requests.
- OpenAI-compatible `/v1/chat/completions` and `/v1/models` endpoints.
- Real SSE token streaming for compatible clients.
- Local tool execution for files, Git, commands, and repository inspection.
- Bounded command execution, output limits, request limits, health checks, and metrics.
- Prometheus-compatible `/metrics` endpoint.
- Multi-chat browser interface with response timing and backend status.
- EXO/RDMA integration point for future multi-Mac MLX clustering.
- Multi-agent AI lab skill with planner, parallel workers, reviewer, reporter, and isolated run artifacts.
- Native behavioral/KV-proxy security guard for prompts, tools, agent runs, and optional MLX telemetry.

## Requirements

- macOS on Apple Silicon
- Rust toolchain
- Python 3.10+ for MLX-LM
- An NVIDIA API key, stored in the macOS Keychain or `NVIDIA_API_KEY`

## Documentation

- [Architecture and routing](docs/ARCHITECTURE.md)
- [Operations, launchd, and troubleshooting](docs/OPERATIONS.md)
- [Testing and verification](docs/TESTING.md)
- [Apple AI bridge](apple-ai-bridge/README.md)
- [Shared trading adapters](ADAPTERS.md)
- [Multi-agent AI lab](skills/multi-agent-ai-lab/SKILL.md)
- [Security guard](security/README.md)
- [KV-first security explained](security/KV_FIRST_SECURITY.md)

## Run

```sh
cargo build --release
./target/release/thomas-ai serve
open http://127.0.0.1:8788
```

For persistent local services, use the launchd scripts in the parent project or configure your own LaunchAgent. The gateway reads the NVIDIA key from Keychain service `NVIDIA_API_KEY`, account `local.local`, by default. No key is stored in this repository.

The current machine deployment runs the private release binary through `com.thomas-ai.gateway` and runs MLX through `com.thomas-ai.mlx`. The launchd definitions live in `~/Library/LaunchAgents`; machine-specific absolute paths are intentionally kept outside this repository.

## API examples

```sh
curl http://127.0.0.1:8788/health
curl http://127.0.0.1:8788/api/status
curl http://127.0.0.1:8788/metrics

curl -N http://127.0.0.1:8788/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"auto","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

## NVIDIA free endpoints

Thomas AI uses an explicit allowlist so arbitrary models returned by NVIDIA cannot be selected accidentally. The default route is Nemotron 3.5 Lightning, easy requests use Nemotron 3.5 Bolt, and difficult requests use DeepSeek V4.1 Flash. The allowlist can be narrowed with `NVIDIA_FREE_MODELS`; it cannot be expanded beyond the verified list in the source.

## Full-access warning

Full-access mode is intentional: the agent can read and write local files and run shell commands as the logged-in macOS user. Run Thomas AI only on a machine and account where this behavior is acceptable. The gateway binds to loopback by default.

## Development

```sh
cargo fmt --all
cargo build --release
cargo clippy --release -- -D warnings
cargo test --release
```

## License

MIT

