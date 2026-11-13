// swift-tools-version: 6.4
import PackageDescription

let package = Package(
    name: "MultiModelAppleAI",
    platforms: [.macOS(.v27)],
    products: [.executable(name: "multi-model-apple-ai", targets: ["MultiModelAppleAI"])],
    targets: [.executableTarget(name: "MultiModelAppleAI")]
)

