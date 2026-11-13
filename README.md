# Multi-Model AI Gateway

Multi-Model AI Gateway is a native Rust AI gateway and LM Studio-style desktop web interface for Apple Silicon. It combines local MLX inference, hosted LLM endpoints, Apple Foundation Models, automatic difficulty and load routing, streaming responses, and full-access local Mac tools.
<img width="1192" height="814" alt="Screenshot 2026-11-14 at 7 46 33 AM" src="https://github.com/user-attachments/assets/8629e105-83e4-411c-bcf4-670274923d74" />

Keywords: multi-model AI gateway, Apple Silicon AI, Rust AI gateway, local LLM, hosted LLM, MLX, NVIDIA NIM API, Nemotron, free AI usage, Apple Foundation Models, OpenAI-compatible API, tool calling, agentic coding, macOS AI.

Article: [How the Multi-Model AI Gateway combines local and hosted AI](https://properproperproper.github.io/multi-model-ai-gateway/)

## Features

- Native Rust gateway optimized for Apple Silicon.
- MLX-LM local inference with automatic launchd service management.
- Hosted routing restricted to catalog-verified free endpoints where configured.
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
- Local Graphify-style repository knowledge graph with HTML visualization, architecture report, and query API.
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
- [Multi-agent AI lab](skills/multi-agent-ai-lab/SKILL.md)
- [Context engineering and local graph](context/README.md)
- [Security guard](security/README.md)
- [KV-first security explained](security/KV_FIRST_SECURITY.md)

## Run

```sh
cargo build --release
./target/release/multi-model-gateway serve
open http://127.0.0.1:8788
```

For persistent local services, configure your own LaunchAgent or run the gateway directly. The gateway reads the NVIDIA key from Keychain service `NVIDIA_API_KEY`, account `local.local`, by default. No key is stored in this repository.

The gateway can run as a standalone release binary. For persistent services, use the service manager appropriate to your operating system.

## API examples

```sh
curl http://127.0.0.1:8788/health
curl http://127.0.0.1:8788/api/status
curl http://127.0.0.1:8788/metrics
open http://127.0.0.1:8788/graph.html

curl -N http://127.0.0.1:8788/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"auto","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

## Local Graphify-style subsystem

The gateway builds a deterministic local graph of the configured repository in Rust. It indexes source, documentation, configuration, and SQL files while excluding Git metadata, build directories, virtual environments, lab artifacts, and credential-like files. The graph contains files, functions/classes/structs/enums/traits, dependencies, imports, definitions, and approximate call relationships.

CLI:

```sh
multi-model-gateway graph scan
multi-model-gateway graph query --query security
```

The scan writes `graphify-out/graph.json`, `graphify-out/GRAPH_REPORT.md`, and `graphify-out/graph.html` locally. The running gateway exposes `GET /api/graph`, `POST /api/graph/query`, `GET /api/graph/report`, and `GET /graph.html`. The gateway uses the graph as bounded repository context for subagent research and code review; it does not upload source code or require a vector database.

## NVIDIA free endpoints

The gateway uses an explicit allowlist so arbitrary models returned by NVIDIA cannot be selected accidentally. The default route is Nemotron 3.5 Lightning, easy requests use Nemotron 3.5 Bolt, and difficult requests use DeepSeek V4.1 Flash. The allowlist can be narrowed with `NVIDIA_FREE_MODELS`; it cannot be expanded beyond the verified list in the source.

## Full-access warning

Full-access mode is intentional: the agent can read and write local files and run shell commands as the logged-in user. Run the gateway only on a machine and account where this behavior is acceptable. The gateway binds to loopback by default.

## Development

```sh
cargo fmt --all
cargo build --release
cargo clippy --release -- -D warnings
cargo test --release
```

## License

MIT

