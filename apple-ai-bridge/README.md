# Apple AI bridge

This optional Swift service exposes Apple’s Foundation Models framework to Multi-Model AI Gateway over JSON Lines. It supports the on-device Apple Foundation Model and Apple Private Cloud Compute on macOS 27 when the required managed entitlement is granted.

## Build

```sh
swift build -c release
./.build/release/multi-model-apple-ai --status
printf '%s\n' '{"prompt":"Summarize this sentence.","mode":"on_device"}' | ./.build/release/multi-model-apple-ai
```

## Private Cloud Compute entitlement

On macOS 27 or later, the bridge calls `PrivateCloudComputeLanguageModel` for
`private_cloud` requests. The executable must be code-signed with the managed
`com.apple.developer.private-cloud-compute` entitlement in
`PrivateCloud.entitlements`. Apple must grant that entitlement to the developer
account; declaring the key locally does not grant access. Without a granted
entitlement, status may report the device as eligible but generation can fail at
runtime, and the bridge returns the error instead of silently using on-device
inference.

After Apple grants access, sign the release executable with the matching
developer identity and provisioning profile:

```sh
codesign --force --sign "Apple Development: YOUR_NAME (YOUR_TEAM_ID)" \
  --entitlements PrivateCloud.entitlements \
  .build/out/Products/Release/multi-model-apple-ai
```

## Protocol

Input:

```json
{"prompt":"Explain this API design.","mode":"on_device","instructions":"Be concise."}
```

Supported modes are `on_device` and `private_cloud`. On macOS 27 or later, `private_cloud` uses `PrivateCloudComputeLanguageModel` directly and reports an explicit error if the entitlement, device, network, or service is not ready. It never silently substitutes the on-device model for an explicit cloud request.

