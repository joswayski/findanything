import AppKit

enum LucideImage {
    static func make(_ icon: LucideIcon, pointSize: CGFloat, accessibilityDescription: String? = nil) -> NSImage {
        let size = NSSize(width: pointSize, height: pointSize)
        let scale = pointSize / LucideIcon.viewbox
        let image = NSImage(size: size, flipped: false) { _ in
            NSGraphicsContext.current?.shouldAntialias = true
            NSColor.black.setStroke()
            for points in icon.paths where !points.isEmpty {
                let path = NSBezierPath()
                path.lineWidth = LucideIcon.strokeWidth * scale
                path.lineCapStyle = .round
                path.lineJoinStyle = .round
                path.move(to: render(points[0], scale: scale, height: pointSize))
                for point in points.dropFirst() {
                    path.line(to: render(point, scale: scale, height: pointSize))
                }
                path.stroke()
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = accessibilityDescription
        return image
    }

    private static func render(_ point: NSPoint, scale: CGFloat, height: CGFloat) -> NSPoint {
        // Generated Lucide points use SVG's y-down coordinate space; AppKit draws y-up.
        NSPoint(x: point.x * scale, y: height - point.y * scale)
    }
}
