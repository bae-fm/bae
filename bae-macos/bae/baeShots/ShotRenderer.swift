import AppKit
import BaeKit
import ScreenCaptureKit
import SwiftUI

/// Renders a `ShotScene` into a deterministic 2x PNG. The view is hosted in a
/// window with the requested appearance, laid out, settled, then captured at
/// a fixed pixel scale regardless of the host machine's display: a scene's
/// view alone from a window past the edge of every display into a manually
/// sized bitmap, without the app ever taking the focus; a scene shown in its
/// window through the window server, which draws the toolbar's glass, from a
/// window brought to the front of the active app.
@MainActor
enum ShotRenderer {
    /// Backing-store scale for every capture. Fixed rather than read from the
    /// screen so a headless or non-Retina host produces the same pixels.
    nonisolated static let scale = 2

    /// What one capture of a scene yields: its pixels, and the elements its
    /// hosted view's accessibility tree lists, read in the same settled state.
    struct Capture {
        let png: Data
        let elements: ShotElementList
    }

    /// The look every shot is drawn in, as the design boards draw: dark,
    /// Midnight, Pink. A scene whose board shows another look stages it itself.
    private static let mode = AppearanceMode.dark
    private static let tone = SurfaceTone.midnight
    private static let accent = AccentChoice.pink

    static func render(_ scene: ShotScene) async throws -> Capture {
        let suite = "fm.bae.appearance-shots"
        guard let defaults = UserDefaults(suiteName: suite) else {
            preconditionFailure("Cannot create screenshot preferences")
        }
        defaults.set(mode.rawValue, forKey: "appearance.mode")
        defaults.set(accent.rawValue, forKey: "appearance.accent")
        defaults.set(tone.rawValue, forKey: "appearance.tone")
        defer { defaults.removePersistentDomain(forName: suite) }
        let stage = scene.stage()
        let rootView = stage.view.appAppearance()
            .defaultAppStorage(defaults)
        let (window, captured) = hostWindow(rootView, for: scene)
        window.appearance = NSAppearance(
            named: mode == .dark ? .darkAqua : .aqua
        )
        window.isReleasedWhenClosed = false
        switch scene.frame {
        case .content:
            window.orderFront(nil)
        case .window:
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            window.makeMain()
        }
        defer {
            window.contentViewController = nil
            window.contentView = nil
            window.close()
        }

        try await settle(host: captured, until: stage.isReady)

        return try await capture(window, host: captured, scene: scene)
    }

    /// The pixels of `host` and the elements below it, once settled.
    private static func capture(
        _ window: NSWindow,
        host: NSView,
        scene: ShotScene
    ) async throws -> Capture {
        let pixels: Data
        switch scene.frame {
        case .content:
            pixels = try png(of: host, size: host.bounds.size, scene: scene)
        case .window:
            try await becomeKey(window, scene: scene)
            pixels = try await windowPNG(
                CGWindowID(window.windowNumber),
                sceneId: scene.id
            )
        }
        // Nothing awaits between the pixels and the tree, so both read the
        // same settled state.
        return Capture(
            png: pixels,
            elements: try ShotElementList.read(
                board: scene.id,
                window: window,
                size: host.bounds.size,
                scale: scale
            )
        )
    }

    /// Wait for `window` to be the key window of the active app, asking for
    /// that again whenever the app or the window loses it: the window server
    /// draws a title bar and toolbar as inactive otherwise, which the app
    /// never shows in use. Fails when another app keeps the focus.
    private static func becomeKey(
        _ window: NSWindow,
        scene: ShotScene
    ) async throws {
        let changes = [
            NSWindow.didBecomeKeyNotification,
            NSWindow.didResignKeyNotification,
            NSApplication.didBecomeActiveNotification,
            NSApplication.didResignActiveNotification,
        ]
        .map { Wait.Notice($0, of: nil) }
        do {
            try await Wait.until(notices: changes) {
                if NSApp.isActive, window.isKeyWindow { return true }
                NSApp.activate(ignoringOtherApps: true)
                window.makeKeyAndOrderFront(nil)
                return false
            }
        }
        catch is Wait.TimedOut {
            throw ShotError.windowNotKey(scene.id)
        }
        // Draw the frame as active and hand it to the window server.
        window.displayIfNeeded()
        CATransaction.flush()
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
        // The window server lists a window a moment after it is ordered in,
        // with an empty frame until it is drawn; a capture sized from that
        // frame is refused as an invalid parameter. Ask again until the
        // window has a frame.
        var shared: SCWindow?
        for _ in 0..<50 {
            let content = try await SCShareableContent.currentProcess
            shared = content.windows.first { $0.windowID == windowID }
            if let shared, !shared.frame.isEmpty, shared.isOnScreen { break }
            try await Task.sleep(for: .milliseconds(100))
        }
        guard let shared else { throw ShotError.windowNotShared(sceneId) }
        guard !shared.frame.isEmpty, shared.isOnScreen else {
            throw ShotError.windowNotDrawn(sceneId, shared.frame)
        }
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
        let rep = NSBitmapImageRep(
            cgImage: try ShotImage.scaled(
                image,
                width: configuration.width,
                height: configuration.height,
                sceneId: sceneId
            )
        )
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
            // Drawn as in the key window of the active app, as the app is
            // seen in use, without making it either: a window of an app that
            // is not active is never key, and draws its controls inactive.
            let host = NSHostingView(
                rootView: rootView.environment(\.controlActiveState, .key)
            )
            host.frame = bounds
            let window = ShotWindow(
                contentRect: NSRect(origin: offscreenOrigin, size: scene.size),
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
            // Ordering a window in plays its zoom-in on a thread of its own,
            // paced by the display it is on; past every display there is
            // none, and the animation would never end.
            window.animationBehavior = .none
            window.contentView = host
            return (window, host)
        case .window(let title, let titleBar):
            let hosting = NSHostingController(rootView: rootView)
            // A window with a hidden title bar has no toolbar either: its
            // content holds what a toolbar would.
            hosting.sceneBridgingOptions =
                titleBar == .shown ? .toolbars : []
            let window = ShotWindow(
                contentRect: bounds,
                styleMask: [.titled, .closable, .miniaturizable, .resizable],
                backing: .buffered,
                defer: false
            )
            if titleBar == .hidden {
                window.styleMask.insert(.fullSizeContentView)
                window.titlebarAppearsTransparent = true
                window.titleVisibility = .hidden
            }
            // Ordering a window in plays its zoom-in, which the window
            // server's capture can catch partway.
            window.animationBehavior = .none
            window.contentViewController = hosting
            window.setContentSize(scene.size)
            window.title = title
            guard let frame = window.contentView?.superview else {
                preconditionFailure("A titled window has a frame view")
            }
            return (window, frame)
        }
    }

    /// A point past the right edge of every display, where a window shows
    /// on none of them.
    private static var offscreenOrigin: NSPoint {
        let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
        return NSPoint(x: right + 1_000, y: 0)
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

    /// Wait until the scene says what it shows has arrived, then until no
    /// frame in `host` moves across an idle of the main run loop, and again
    /// if coming to rest asked for more. The awaits are what let the
    /// scene's main-actor work run; blocking the main thread would starve
    /// it.
    ///
    /// An idle can come while main-actor work is still queued, so a tree
    /// can read the same across one before a chain of main-actor steps that
    /// moves no frame has done anything; a scene whose reads start that way
    /// names in `isReady` what they bring, such as a store's list.
    private static func settle(
        host: NSView,
        until isReady: @MainActor () -> Bool
    ) async throws {
        while true {
            try await Wait.until { isReady() }
            try await Wait.untilSteady {
                host.layoutSubtreeIfNeeded()
                return frames(below: host)
            }
            if isReady() { return }
        }
    }

    /// The frames of every view below `view`, depth first.
    private static func frames(below view: NSView) -> [CGRect] {
        view.subviews.flatMap { [$0.frame] + frames(below: $0) }
    }
}

enum ShotError: Error, CustomStringConvertible {
    case missingOutputDir
    case bitmapAllocationFailed(String)
    case pngEncodingFailed(String)
    case windowNotShared(String)
    case windowNotDrawn(String, CGRect)
    case windowNotKey(String)
    case unknownSelection(String, String)

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
        case .windowNotDrawn(let id, let frame):
            "the window server shows scene \(id)'s window with frame "
                + "\(frame), off the screen or empty"
        case .windowNotKey(let id):
            "another app kept the focus, so scene \(id) would draw inactive"
        case .unknownSelection(let kind, let name):
            "no \(kind) matches \(name)"
        }
    }
}

private final class ShotWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
