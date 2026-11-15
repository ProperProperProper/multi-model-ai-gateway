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

Supported modes are `on_device` and `private_cloud`. The public macOS 26 SDK does not expose a Private Cloud Compute model, so `private_cloud` requests safely fall back to the on-device Foundation Model and report that fallback.

