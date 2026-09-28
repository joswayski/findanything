import AppKit
import CoreText

private final class FontBundleMarker: NSObject {}

enum Fonts {
    private static let registered: Void = {
        // App bundles use Resources; bare SwiftPM builds stage fonts beside the executable.
        let executable = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent()
        // XCTest loads our code in <bin>/<test>.xctest, not the xctest runner's directory.
        let testBin = Bundle(for: FontBundleMarker.self).bundleURL.deletingLastPathComponent()
        let roots = [Bundle.main.resourceURL, executable, testBin].compactMap { $0 }
        for name in ["Inter-Regular", "Inter-SemiBold"] {
            guard let url = roots.map({ $0.appendingPathComponent("fonts/\(name).ttf") })
                .first(where: { FileManager.default.fileExists(atPath: $0.path) }) else {
                preconditionFailure("Missing bundled UI font: \(name)")
            }
            CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        }
    }()

    static func regular(_ size: CGFloat) -> NSFont {
        _ = registered
        return NSFont(name: "Inter-Regular", size: size)!
    }

    static func semibold(_ size: CGFloat) -> NSFont {
        _ = registered
        return NSFont(name: "Inter-SemiBold", size: size)!
    }
}
