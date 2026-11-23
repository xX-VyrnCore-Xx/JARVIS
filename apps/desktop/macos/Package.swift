// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "JARVIS",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "jarvis-desktop", targets: ["JARVIS"])],
    targets: [.executableTarget(name: "JARVIS")]
)
