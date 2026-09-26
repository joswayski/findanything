// swift-tools-version: 5.9
import PackageDescription
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let libraryDirectory = ProcessInfo.processInfo.environment["FINDANYTHING_LIB_DIR"]
    ?? root.appendingPathComponent("../../../target/release").standardizedFileURL.path

let package = Package(
    name: "FindAnythingNative",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "FindAnythingNative", targets: ["FindAnythingNative"])],
    targets: [
        .systemLibrary(name: "CFindAnything", path: "Sources/CFindAnything"),
        .executableTarget(name: "FindAnythingNative", dependencies: ["CFindAnything"], linkerSettings: [
            .unsafeFlags(["-L", libraryDirectory]), .linkedLibrary("findanything_ffi"),
            .linkedFramework("AppKit"), .linkedFramework("Carbon"),
            .linkedFramework("Security"), .linkedFramework("SystemConfiguration"),
            .linkedFramework("CoreFoundation"), .linkedLibrary("c++"), .linkedLibrary("resolv")
        ]),
        .testTarget(name: "FindAnythingNativeTests", dependencies: ["FindAnythingNative"])
    ]
)
