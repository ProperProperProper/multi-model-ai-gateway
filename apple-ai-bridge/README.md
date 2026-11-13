# Apple AI bridge

This optional Swift service exposes Apple’s Foundation Models framework to Multi-Model AI Gateway over JSON Lines. It supports the on-device Apple Foundation Model and Apple Private Cloud Compute when the Mac, OS, account, and entitlement make that model available.

## Build

```sh
swift build -c release
./.build/release/multi-model-apple-ai --status
printf '%s\n' '{"prompt":"Summarize this sentence.","mode":"on_device"}' | ./.build/release/multi-model-apple-ai
```

## Protocol

Input:

```json
{"prompt":"Explain this API design.","mode":"on_device","instructions":"Be concise."}
```

Supported modes are `on_device` and `private_cloud`. Private Cloud Compute is optional and is used only when Apple reports it as available; otherwise Multi-Model AI Gateway continues using MLX or the verified free NVIDIA pool.

