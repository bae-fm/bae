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
/// non-activating window made key on the `ShotDisplay`, without the app
/// taking the focus; an alert it shows is a non-activating panel too.
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

    /// `scene` captured; a scene shown in its window is drawn on `display`.
    static func render(
        _ scene: ShotScene,
        on display: ShotDisplay
    ) async throws -> Capture {
        let suite = "fm.bae.appearance-shots"
        guard let defaults = UserDefaults(suiteName: suite) else {
            preconditionFailure("Cannot create screenshot preferences")
        }
        defaults.set(mode.rawValue, forKey: "appearance.mode")
        defaults.set(accent.rawValue, forKey: "appearance.accent")
        defaults.set(tone.rawValue, forKey: "appearance.tone")
        defer { defaults.removePersistentDomain(forName: suite) }
        let stage = try scene.stage()
        // A view that opens or moves after the window shows, such as an album
        // grid's detail opening on its place's first answer, lands at once,
        // so the capture never holds a frame partway through.
        let rootView = stage.view.appAppearance()
            .defaultAppStorage(defaults)
            .transaction {
                $0.animation = nil
                $0.disablesAnimations = true
            }
        let (window, captured) = try hostWindow(
            rootView,
            for: scene,
            size: try frameSize(of: scene),
            on: display
        )
        window.appearance = NSAppearance(
            named: mode == .dark ? .darkAqua : .aqua
        )
        window.isReleasedWhenClosed = false
        // Close the window however the render ends, a failure to place it
        // among them: the scene's view raises its alerts once it is in the
        // window, shown or not, and an alert left up is listed and captured
        // with every scene after it.
        defer {
            window.contentViewController = nil
            window.contentView = nil
            window.close()
        }
        switch scene.frame {
        case .content, .component:
            // The renderer makes the window key without activating the app.
            // The window sits past every display, and a focused field draws
            // its ring only in a key window.
            window.orderFront(nil)
            window.makeKey()
        case .window:
            try place(window, on: display, scene: scene)
            // The window is a non-activating panel, key without the app
            // taking the focus from the app that holds it.
            window.makeKeyAndOrderFront(nil)
            window.makeMain()
        }

        try await settle(
            host: captured,
            until: stage.isReady,
            drawn: stage.isDrawn
        )

        return try await capture(
            window,
            host: captured,
            scene: scene,
            on: display
        )
    }

    /// The pixels of `host` and the elements below it, once settled.
    private static func capture(
        _ window: NSWindow,
        host: NSView,
        scene: ShotScene,
        on display: ShotDisplay
    ) async throws -> Capture {
        let pixels: Data
        switch scene.frame {
        case .content, .component:
            pixels = try png(of: host, size: host.bounds.size, scene: scene)
        case .window:
            // The app shows the alerts a few run loop passes after AppKit
            // orders the window in, so the renderer waits for them before it
            // makes the top one key.
            let alerts = try await alertPanels()
            try await becomeKey(window, scene: scene)
            try await shownOnScreen(
                [window] + AlertWindows.shared.panels,
                scene: scene
            )
            pixels = try await windowPNG(
                CGWindowID(window.windowNumber),
                alerts: alerts,
                on: display.id,
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

    /// The window `rootView` is shown in for `scene`, and the view the
    /// capture settles: the view alone in a borderless window, or for a
    /// scene shown in its window, the whole frame, title bar and toolbar
    /// included, the way the app's window controller hosts it.
    private static func hostWindow(
        _ rootView: some View,
        for scene: ShotScene,
        size: CGSize,
        on display: ShotDisplay
    ) throws -> (NSWindow, NSView) {
        let bounds = NSRect(origin: .zero, size: size)
        switch scene.frame {
        case .content, .component:
            // Drawn as in the key window of the active app, as the app is
            // seen in use, without making it either: a window of an app that
            // is not active is never key, and draws its controls inactive.
            let host = NSHostingView(
                rootView: rootView.environment(\.controlActiveState, .key)
            )
            host.frame = bounds
            let window = contentWindow(for: scene, size: size)
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
            // Made on the shot display, so AppKit sizes it against that
            // display rather than the Mac's own.
            let origin = try display.screen().visibleFrame
                .origin
            let window = shownWindow(for: scene, at: origin)
            if titleBar == .hidden {
                // SwiftUI's hidden-title-bar window lets its content reach
                // the top edge; the hosting view otherwise keeps the title
                // bar's height as a safe area and shifts the content down.
                hosting.safeAreaRegions = []
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

    /// The borderless window that holds a scene's view alone, past the edge
    /// of every display: a panel that does not activate the app when the
    /// scene needs its window key.
    private static func contentWindow(
        for scene: ShotScene,
        size: CGSize
    ) -> NSWindow {
        let contentRect = NSRect(origin: offscreenOrigin, size: size)
        guard scene.keyWindow else {
            return ShotWindow(
                contentRect: contentRect,
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
        }
        let panel = ShotPanel(
            contentRect: contentRect,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        // A panel hides when its app is not active, which this app never is.
        panel.hidesOnDeactivate = false
        return panel
    }

    /// The empty window of a scene shown in its window: a panel that does
    /// not activate the app.
    private static func shownWindow(
        for scene: ShotScene,
        at origin: NSPoint
    ) -> NSWindow {
        let contentRect = NSRect(origin: origin, size: scene.size)
        let titled: NSWindow.StyleMask = [
            .titled, .closable, .miniaturizable, .resizable,
        ]
        let panel = ShotPanel(
            contentRect: contentRect,
            styleMask: titled.union(.nonactivatingPanel),
            backing: .buffered,
            defer: false
        )
        // A panel hides when its app is not active, which this app never is.
        panel.hidesOnDeactivate = false
        return panel
    }

    /// The size a scene is drawn at: a component's is its board's, which
    /// the view is proposed and the shot is cut to; any other scene's is its
    /// own.
    private static func frameSize(of scene: ShotScene) throws -> CGSize {
        switch scene.frame {
        case .component: try ShotBoardSizes.size(of: scene.id)
        case .content, .window: scene.size
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
    /// names in `isReady` what they bring, such as a store's list. What a view
    /// builds from a read it makes itself, which no store publishes, is
    /// named in `isDrawn`, read from the laid-out tree.
    private static func settle(
        host: NSView,
        until isReady: @MainActor () -> Bool,
        drawn isDrawn: @MainActor (NSView) -> Bool
    ) async throws {
        while true {
            try await Wait.until { isReady() }
            try await Wait.untilLaidOut(host) { isDrawn(host) }
            try await Wait.untilSteady {
                host.layoutSubtreeIfNeeded()
                return frames(below: host)
            }
            if isReady(), isDrawn(host) { return }
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
    /// The scene's window never became key; `focus` is the app macOS named
    /// as the front app when the wait ended.
    case windowNotKey(String, focus: FocusHolder)
    case noScreen(String)
    case noShotDisplay(String)
    case noContentView(String)
    case windowTooLarge(
        String,
        content: CGSize,
        window: CGSize,
        shown: CGSize,
        room: CGSize
    )
    case unknownSelection(String, String)
    case missingBoardSizes
    case noBoardSize(String)

    var description: String {
        switch self {
        case .missingOutputDir:
            "BUILDS_SHOTS_OUT is unset — pass the output directory through "
                + "TEST_RUNNER_BUILDS_SHOTS_OUT."
        case .bitmapAllocationFailed(let id):
            "failed to allocate the capture bitmap for scene \(id)"
        case .pngEncodingFailed(let id):
            "failed to PNG-encode scene \(id)"
        case .windowNotShared(let id):
            "the window server shares no window for scene \(id)"
        case .windowNotDrawn(let id, let frame):
            "the window server shows scene \(id)'s window with frame "
                + "\(frame), off the screen or empty"
        case .windowNotKey(let id, .other(.some(let name), let pid)):
            "\(name) (pid \(pid)) kept the focus, so scene \(id) would draw "
                + "inactive"
        case .windowNotKey(let id, .other(nil, let pid)):
            "an app with no name (pid \(pid)) kept the focus, so scene \(id) "
                + "would draw inactive"
        case .windowNotKey(let id, .thisApp):
            "the shot test is the front app, but scene \(id)'s window never "
                + "became key, so it would draw inactive"
        case .windowNotKey(let id, .noApp):
            "macOS names no front app, and scene \(id)'s window never became "
                + "key, so it would draw inactive"
        case .noScreen(let id):
            "ScreenCaptureKit shares no shot display for scene \(id)'s window"
        case .noContentView(let id):
            "scene \(id)'s window has no content view"
        case .noShotDisplay(let why):
            "the renderer has no shot display for the scenes shown in their "
                + "windows: \(why)"
        case .windowTooLarge(
            let id,
            let content,
            let window,
            let shown,
            let room
        ):
            "scene \(id) asks for \(content) points of content, and its "
                + "window of \(window) points shows \(shown) of it, on a shot "
                + "display with a free area of \(room) points"
        case .missingBoardSizes:
            "BUILDS_SHOTS_BOARD_SIZES is unset: builds shots writes the "
                + "boards' sizes and passes the file's path through "
                + "TEST_RUNNER_BUILDS_SHOTS_BOARD_SIZES."
        case .noBoardSize(let id):
            "the boards' sizes list none for component scene \(id)"
        case .unknownSelection(let kind, let name):
            "no \(kind) matches \(name)"
        }
    }
}

private final class ShotWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    /// The window keeps the size the scene gives it. AppKit fits a window to
    /// a display as it sizes it and orders it in, and can fit it to the Mac's
    /// own display rather than the shot display the window is on; `place`
    /// refuses a window that does not fit the shot display instead.
    override func constrainFrameRect(
        _ frameRect: NSRect,
        to screen: NSScreen?
    ) -> NSRect { frameRect }
}

/// The window of a scene shown in its window, or of a scene that needs its
/// view in a key window. A panel that does not activate the app is key, and draws its title bar and toolbar as active,
/// while another app holds the focus: a window of an app that is not active
/// is never key.
private final class ShotPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    /// The panel keeps the size the scene gives it, as `ShotWindow` does.
    override func constrainFrameRect(
        _ frameRect: NSRect,
        to screen: NSScreen?
    ) -> NSRect { frameRect }
}
