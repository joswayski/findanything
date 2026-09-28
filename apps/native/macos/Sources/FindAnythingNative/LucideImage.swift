import AppKit

enum LucideImage {
    static func make(_ icon: LucideIcon, pointSize: CGFloat, accessibilityDescription: String? = nil) -> NSImage {
        let size = NSSize(width: pointSize, height: pointSize)
        let image = NSImage(size: size, flipped: false) { rect in
            let scale = min(rect.width, rect.height) / LucideIcon.viewbox
            NSGraphicsContext.current?.shouldAntialias = true
            NSColor.black.setStroke()
            for points in icon.paths where !points.isEmpty {
                let path = NSBezierPath()
                path.lineWidth = LucideIcon.strokeWidth * scale
                path.lineCapStyle = .round
                path.lineJoinStyle = .round
                path.move(to: render(points[0], scale: scale, bounds: rect))
                for point in points.dropFirst() {
                    path.line(to: render(point, scale: scale, bounds: rect))
                }
                path.stroke()
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = accessibilityDescription
        return image
    }

    private static func render(_ point: NSPoint, scale: CGFloat, bounds: NSRect) -> NSPoint {
        // AppKit can request a scaled, nonzero destination. Center the square
        // SVG view box there and convert its y-down coordinates to Cocoa y-up.
        NSPoint(
            x: bounds.midX + (point.x - LucideIcon.viewbox / 2) * scale,
            y: bounds.midY + (LucideIcon.viewbox / 2 - point.y) * scale
        )
    }
}
