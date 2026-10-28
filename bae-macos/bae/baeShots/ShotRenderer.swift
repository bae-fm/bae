import AppKit
import BaeKit
import ScreenCaptureKit
import SwiftUI

/// Renders a `ShotScene` offscreen into a deterministic 2x PNG. The view is
/// hosted in a window with the requested appearance, laid out, settled, then
/// captured at a fixed pixel scale regardless of the host machine's display:
/// into a manually sized bitmap, or for a scene shown in its window, through
/// the window server, which draws the toolbar's glass.
@MainActor
enum ShotRenderer {
    /// Backing-store scale for every capture. Fixed rather than read from the
    /// screen so a headless or non-Retina host produces the same pixels.
    nonisolated static let scale = 2

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
        let rootView = scene.makeView().appAppearance()
            .defaultAppStorage(defaults)
        let (window, captured) = hostWindow(rootView, for: scene)
        window.appearance = NSAppearance(
            named: mode == .dark ? .darkAqua : .aqua
        )
        window.isReleasedWhenClosed = false
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        window.makeMain()
        defer {
            window.contentViewController = nil
            window.contentView = nil
            window.close()
        }

        await settle(host: captured)

        switch scene.frame {
        case .content:
            return try png(
                of: captured,
                size: captured.bounds.size,
                scene: scene
            )
        case .window:
            try await becomeKey(window, scene: scene)
            return try await windowPNG(
                CGWindowID(window.windowNumber),
                sceneId: scene.id
            )
        }
    }

    /// Wait for `window` to be the key window of the active app, asking for
    /// that again as it waits: the window server draws a title bar and
    /// toolbar as inactive otherwise, which the app never shows in use. Fails
    /// when another app keeps the focus.
    private static func becomeKey(
        _ window: NSWindow,
        scene: ShotScene
    ) async throws {
        for _ in 0..<50 {
            if NSApp.isActive, window.isKeyWindow {
                // One more pass for the frame to redraw as active.
                try? await Task.sleep(nanoseconds: 200_000_000)
                return
            }
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
        throw ShotError.windowNotKey(scene.id)
    }

    /// The whole window as the window server composes it, as PNG bytes: a
    /// window's toolbar draws its controls on glass, which a view's own
    /// drawing leaves out, so the window is captured the way the screen
    /// shows it. A process may capture its own windows without the screen
    /// recording permission. ScreenCaptureKit's results aren't `Sendable`,
    /// so the capture runs and encodes off the main actor and hands back
    /// only the PNG bytes.
    @concurrent
    private nonisolated static func windowPNG(
        _ windowID: CGWindowID,
        sceneId: String
    ) async throws -> Data {
        let content = try await SCShareableContent.currentProcess
        guard
            let shared = content.windows.first(where: {
                $0.windowID == windowID
            })
        else { throw ShotError.windowNotShared(sceneId) }
        let filter = SCContentFilter(desktopIndependentWindow: shared)
        let configuration = SCStreamConfiguration()
        configuration.width = Int(filter.contentRect.width) * scale
        configuration.height = Int(filter.contentRect.height) * scale
        configuration.ignoreShadowsSingleWindow = true
        configuration.showsCursor = false
        let image = try await SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        )
        let rep = NSBitmapImageRep(cgImage: image)
        rep.size = filter.contentRect.size
        guard let data = rep.representation(using: .png, properties: [:])
        else {
            throw ShotError.pngEncodingFailed(sceneId)
        }
        return data
    }

    /// The window `rootView` is shown in for `scene`, and the view the
    /// capture settles: the view alone in a borderless window, or for a
    /// scene shown in its window, the whole frame, title bar and toolbar
    /// included, the way the app's window controller hosts it.
    private static func hostWindow(
        _ rootView: some View,
        for scene: ShotScene
    ) -> (NSWindow, NSView) {
        let bounds = NSRect(origin: .zero, size: scene.size)
        switch scene.frame {
        case .content:
            let host = NSHostingView(rootView: rootView)
            host.frame = bounds
            let window = ShotWindow(
                contentRect: bounds,
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
            window.contentView = host
            return (window, host)
        case .window(let title):
            let hosting = NSHostingController(rootView: rootView)
            hosting.sceneBridgingOptions = .toolbars
            let window = ShotWindow(
                contentRect: bounds,
                styleMask: [.titled, .closable, .miniaturizable, .resizable],
                backing: .buffered,
                defer: false
            )
            window.contentViewController = hosting
            window.setContentSize(scene.size)
            window.title = title
            guard let frame = window.contentView?.superview else {
                preconditionFailure("A titled window has a frame view")
            }
            return (window, frame)
        }
    }

    /// `host`'s pixels at `size` points and `scale`, over the window
    /// background, as PNG bytes.
    private static func png(
        of host: NSView,
        size: CGSize,
        scene: ShotScene
    ) throws -> Data {
        let bounds = NSRect(origin: .zero, size: size)
        guard
            let rep = NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: Int(size.width) * scale,
                pixelsHigh: Int(size.height) * scale,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 0,
                bitsPerPixel: 0
            )?
            .retagging(with: .sRGB)
        else {
            throw ShotError.bitmapAllocationFailed(scene.id)
        }
        // Point size on a 2x-pixel backing → cacheDisplay renders at 2x.
        rep.size = size
        host.cacheDisplay(in: bounds, to: rep)
        // The app draws the window's background under what the view leaves
        // unpainted; the capture alone would leave it transparent.
        let composed = try WindowBackdrop.composite(rep, of: host, size: size)

        guard let data = composed.representation(using: .png, properties: [:])
        else {
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
    case windowNotShared(String)
    case windowNotKey(String)

    var description: String {
        switch self {
        case .missingOutputDir:
            "BAE_SHOTS_OUT is unset — pass the output directory through "
                + "TEST_RUNNER_BAE_SHOTS_OUT."
        case .bitmapAllocationFailed(let id):
            "failed to allocate the capture bitmap for scene \(id)"
        case .pngEncodingFailed(let id):
            "failed to PNG-encode scene \(id)"
        case .windowNotShared(let id):
            "the window server shares no window for scene \(id)"
        case .windowNotKey(let id):
            "another app kept the focus, so scene \(id) would draw inactive"
        }
    }
}

private final class ShotWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
