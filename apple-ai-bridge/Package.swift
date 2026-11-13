// swift-tools-version: 6.3
import PackageDescription

let package = Package(
    name: "MultiModelAppleAI",
    platforms: [.macOS(.v26)],
    products: [.executable(name: "multi-model-apple-ai", targets: ["MultiModelAppleAI"])],
    targets: [.executableTarget(name: "MultiModelAppleAI")]
)

