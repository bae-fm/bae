import AppKit
import BaeKit
import SwiftUI

/// Renders a `ShotScene` offscreen into a deterministic 2x PNG. The view is
/// hosted in a window with the requested appearance, laid out, settled, then
/// captured into a manually sized bitmap so the pixel scale is fixed regardless
/// of the host machine's display.
@MainActor
enum ShotRenderer {
    /// Backing-store scale for every capture. Fixed rather than read from the
    /// screen so a headless or non-Retina host produces the same pixels.
    static let scale = 2

    static func renderPNG(
        _ scene: ShotScene,
        mode: AppearanceMode,
        tone: SurfaceTone,
        accent: AccentChoice
    ) async throws -> Data {
        let suite = "fm.bae.appearance-shots"
        guard let defaults = UserDefaults(suiteName: suite) else {
            preconditionFailure("Cannot create screenshot preferences")
        }
        defaults.set(mode.rawValue, forKey: "appearance.mode")
        defaults.set(accent.rawValue, forKey: "appearance.accent")
        defaults.set(tone.rawValue, forKey: "appearance.tone")
        defer { defaults.removePersistentDomain(forName: suite) }
        let bounds = NSRect(origin: .zero, size: scene.size)
        let host = NSHostingView(
            rootView: scene.makeView().defaultAppStorage(defaults)
                .appearance(mode: mode, accent: accent, tone: tone)
        )
        host.frame = bounds

        let window = ShotWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.appearance = NSAppearance(
            named: mode == .dark ? .darkAqua : .aqua
        )
        window.isReleasedWhenClosed = false
        window.contentView = host
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        defer {
            window.contentView = nil
            window.close()
        }

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

private final class ShotWindow: NSWindow {
    override var canBecomeKey: Bool { true }
}
