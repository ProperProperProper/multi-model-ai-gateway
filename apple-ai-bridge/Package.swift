// swift-tools-version: 6.4
import PackageDescription

let package = Package(
    name: "ThomasAppleAI",
    platforms: [.macOS(.v27)],
    products: [.executable(name: "thomas-apple-ai", targets: ["ThomasAppleAI"])],
    targets: [.executableTarget(name: "ThomasAppleAI")]
)

