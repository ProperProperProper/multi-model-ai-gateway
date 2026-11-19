# Apple AI bridge

This optional Swift service exposes Apple’s Foundation Models framework to Multi-Model AI Gateway over JSON Lines. It supports the on-device Apple Foundation Model and accepts an explicit `private_cloud` compatibility mode. The public macOS SDK currently does not expose a callable Apple Private Cloud Compute model, so that mode reports cloud unavailability and safely falls back on-device.

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

Supported modes are `on_device` and `private_cloud`. The public macOS 26 SDK does not expose a Private Cloud Compute model, so `private_cloud` requests safely fall back to the on-device Foundation Model and report that fallback.

