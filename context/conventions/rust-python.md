# Implementation conventions

Use native Rust for gateway, routing, concurrency, bounded process execution, and HTTP behavior. Use Python for MLX, datasets, model training, and adapters where the native Apple framework requires it. Keep interfaces JSON/OpenAI-compatible. Add focused tests and run format, build, test, and health checks after changes.
