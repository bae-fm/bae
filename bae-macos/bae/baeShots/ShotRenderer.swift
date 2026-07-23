import AppKit
import SwiftUI

/// Renders a `ShotScene` offscreen into a deterministic 2x PNG. The view is
/// hosted in a borderless dark-appearance window (matching the app's pinned
/// dark scheme), laid out, given time for its async content to settle, then
/// captured into a manually sized bitmap so the pixel scale is fixed regardless
/// of the host machine's display.
@MainActor
enum ShotRenderer {
    /// Backing-store scale for every capture. Fixed rather than read from the
    /// screen so a headless or non-Retina host produces the same pixels.
    static let scale = 2

    static func renderPNG(_ scene: ShotScene) async throws -> Data {
        let bounds = NSRect(origin: .zero, size: scene.size)
        let host = NSHostingView(rootView: scene.makeView())
        host.frame = bounds

        let window = NSWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = host
        window.makeKeyAndOrderFront(nil)

        await settle(host: host)

        guard
            let rep = NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: Int(scene.size.width) * scale,
                pixelsHigh: Int(scene.size.height) * scale,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 0,
                bitsPerPixel: 0
            )
        else {
            throw ShotError.bitmapAllocationFailed(scene.id)
        }
        // Point size on a 2x-pixel backing → cacheDisplay renders at 2x.
        rep.size = scene.size
        host.cacheDisplay(in: bounds, to: rep)

        guard let data = rep.representation(using: .png, properties: [:]) else {
            throw ShotError.pngEncodingFailed(scene.id)
        }
        return data
    }

    /// Suspend the capturing task so SwiftUI commits layout and any
    /// `.task`-driven or eager-load work (the album grid's first page, seeded
    /// release details) runs to completion on the main actor before the
    /// capture. Blocking the main thread here would starve those cooperative
    /// tasks — the awaits are what let them run.
    private static func settle(host: NSView) async {
        host.layoutSubtreeIfNeeded()
        for _ in 0..<20 {
            await Task.yield()
        }
        try? await Task.sleep(nanoseconds: 1_500_000_000)
        host.layoutSubtreeIfNeeded()
    }
}

enum ShotError: Error, CustomStringConvertible {
    case missingOutputDir
    case bitmapAllocationFailed(String)
    case pngEncodingFailed(String)

    var description: String {
        switch self {
        case .missingOutputDir:
            "BAE_SHOTS_OUT is unset — pass the output directory through "
                + "TEST_RUNNER_BAE_SHOTS_OUT."
        case .bitmapAllocationFailed(let id):
            "failed to allocate the capture bitmap for scene \(id)"
        case .pngEncodingFailed(let id):
            "failed to PNG-encode scene \(id)"
        }
    }
}
