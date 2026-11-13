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
            // Key without being ordered to the front: the window sits past
            // every display, and a field a scene focuses draws its ring only
            // in a key window.
            window.orderFront(nil)
            window.makeKey()
        case .window:
            try place(window, scene: scene)
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
                sheets: try await sheets(of: window, scene: scene),
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

    /// Put `window` wholly on the display with the most room, at its top left:
    /// the screen capture takes only what is on a display, and a window past
    /// a display's edge, or a title bar pushed up under the menu bar, would
    /// be cut off or refused.
    private static func place(_ window: NSWindow, scene: ShotScene) throws {
        guard
            let room = NSScreen.screens.map(\.visibleFrame)
                .max(by: {
                    $0.height * $0.width < $1.height * $1.width
                })
        else { throw ShotError.noScreen(scene.id) }
        guard
            window.frame.width <= room.width,
            window.frame.height <= room.height
        else { throw ShotError.windowTooLarge(scene.id, window.frame.size) }
        window.setFrameOrigin(
            NSPoint(x: room.minX, y: room.maxY - window.frame.height)
        )
    }

    /// Wait for `window`, or an alert over it, to be the key window of the active app, asking for
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
                // An alert over the window holds the key, and the window draws
                // as active under it.
                if NSApp.isActive,
                    window.isKeyWindow
                        || AlertWindows.shared.panels.contains(
                            where: \.isKeyWindow
                        )
                {
                    return true
                }
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
            // The window's size is the scene's, whatever the content asks for.
            hosting.sizingOptions = []
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
            if titleBar == .hidden {
                // The content reaches under the title bar, so the frame is
                // the scene's size; sizing the content would add the title
                // bar's height below it, an empty strip in the capture.
                window.setFrame(
                    NSRect(origin: window.frame.origin, size: scene.size),
                    display: false
                )
            }
            else {
                window.setContentSize(scene.size)
            }
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
    case noScreen(String)
    case windowTooLarge(String, CGSize)
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
        case .noScreen(let id):
            "no display to place scene \(id)'s window against"
        case .windowTooLarge(let id, let size):
            "scene \(id)'s window, \(size), is larger than any display's free area"
        case .unknownSelection(let kind, let name):
            "no \(kind) matches \(name)"
        }
    }
}

private final class ShotWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
