import AppKit
import BaeKit
import ColorSync
import os.log

private let frameLogger = Logger.bae("WindowFrameAutosave")

/// Saves one window's frame relative to its display. AppKit also keeps a
/// frame under the autosave name so it can fit the window to an available
/// display when the original display is gone.
@MainActor
struct WindowFrameAutosave {
    let name: NSWindow.FrameAutosaveName
    let defaults: UserDefaults
    let bugReporter: any BridgeBugReporter
    let showError: @MainActor (BridgeError) -> Void
    private let identifyDisplay: @MainActor (NSScreen) -> UUID?

    init(
        name: NSWindow.FrameAutosaveName,
        defaults: UserDefaults,
        bugReporter: any BridgeBugReporter,
        showError: @escaping @MainActor (BridgeError) -> Void,
        identifyDisplay: @escaping @MainActor (NSScreen) -> UUID? =
            WindowFrameAutosave.displayID
    ) {
        self.name = name
        self.defaults = defaults
        self.bugReporter = bugReporter
        self.showError = showError
        self.identifyDisplay = identifyDisplay
    }

    private var key: String { "bae.window-placement.\(name)" }

    func save(_ window: NSWindow) {
        guard let screen = window.screen else {
            frameLogger.debug("Cannot save \(name) without a display")
            return
        }
        guard let display = identifyDisplay(screen) else {
            reportBug(
                operation: "Remember a window's display",
                detail: "AppKit gave no display identifier for \(name)"
            )
            return
        }
        let frame = window.frame.offsetBy(
            dx: -screen.frame.minX,
            dy: -screen.frame.minY
        )
        defaults.set(
            ["display": display.uuidString, "frame": NSStringFromRect(frame)],
            forKey: key
        )
    }

    private func reportBug(operation: String, detail: String) {
        showError(
            frameLogger.reportBug(
                operation: operation,
                detail: detail,
                to: bugReporter
            )
        )
    }

    private struct SavedFrame {
        let display: UUID
        /// Coordinates relative to the display.
        let frame: CGRect

        init?(value: Any) {
            guard let fields = value as? [String: String],
                let id = fields["display"], let display = UUID(uuidString: id),
                let encoded = fields["frame"]
            else { return nil }
            let frame = NSRectFromString(encoded)
            guard
                [frame.minX, frame.minY, frame.width, frame.height]
                    .allSatisfy(\.isFinite), !frame.isEmpty
            else { return nil }
            self.display = display
            self.frame = frame
        }
    }

    private func readPlacement() -> SavedFrame? {
        guard let value = defaults.object(forKey: key) else { return nil }
        guard let saved = SavedFrame(value: value)
        else {
            reportBug(
                operation: "Read a saved window frame",
                detail: "The saved placement for \(name) is invalid"
            )
            return nil
        }
        return saved
    }

    private static func displayID(_ screen: NSScreen) -> UUID? {
        guard let id = screen.displayNumber,
            let uuid = CGDisplayCreateUUIDFromDisplayID(id)?.takeRetainedValue()
        else { return nil }
        let bytes = CFUUIDGetUUIDBytes(uuid)
        return UUID(
            uuid: (
                bytes.byte0, bytes.byte1, bytes.byte2, bytes.byte3,
                bytes.byte4, bytes.byte5, bytes.byte6, bytes.byte7,
                bytes.byte8, bytes.byte9, bytes.byte10, bytes.byte11,
                bytes.byte12, bytes.byte13, bytes.byte14, bytes.byte15
            )
        )
    }
}

extension WindowFrameAutosave {
    /// The displays at the moment the window opens.
    struct Opening {
        let autosave: WindowFrameAutosave
        let preferredScreen: NSScreen?
        let screens: [NSScreen]

        @MainActor
        func place(_ window: NSWindow, contentSized: Bool) {
            guard let firstScreen = preferredScreen ?? screens.first else {
                frameLogger.debug(
                    "Cannot place \(autosave.name) without a display"
                )
                return
            }
            let initialSize = window.frame.size
            let registered = window.setFrameAutosaveName(autosave.name)
            if !registered {
                autosave.reportBug(
                    operation: "Register a window frame autosave name",
                    detail: "AppKit rejected the name \(autosave.name)"
                )
            }
            guard let saved = autosave.readPlacement() else {
                Self.center(window, size: initialSize, on: firstScreen)
                return
            }
            let contentFrameSize = contentSized ? initialSize : nil
            if let target = restoredTarget(
                window,
                from: saved,
                registered: registered
            ) {
                Self.restore(
                    window,
                    frame: target.frame,
                    size: contentFrameSize ?? target.frame.size,
                    on: target.screen
                )
            }
            else {
                Self.center(
                    window,
                    size: contentFrameSize ?? saved.frame.size,
                    on: firstScreen
                )
            }
        }

        @MainActor
        private func restoredTarget(
            _ window: NSWindow,
            from saved: SavedFrame,
            registered: Bool
        ) -> (frame: CGRect, screen: NSScreen)? {
            if let screen = screen(matching: saved.display) {
                return (
                    saved.frame.offsetBy(
                        dx: screen.frame.minX,
                        dy: screen.frame.minY
                    ),
                    screen
                )
            }
            frameLogger.debug(
                "The saved display for \(autosave.name) is unavailable"
            )
            guard registered, window.setFrameUsingName(autosave.name),
                let screen = window.screen
            else {
                frameLogger.warning(
                    "AppKit could not restore a saved frame for \(autosave.name)"
                )
                return nil
            }
            return (window.frame, screen)
        }

        @MainActor
        private func screen(matching display: UUID) -> NSScreen? {
            screens.first { screen in
                guard let id = autosave.identifyDisplay(screen) else {
                    autosave.reportBug(
                        operation: "Restore a window's display",
                        detail:
                            "AppKit gave no display identifier while restoring \(autosave.name)"
                    )
                    return false
                }
                return id == display
            }
        }

        @MainActor
        private static func restore(
            _ window: NSWindow,
            frame: CGRect,
            size: CGSize,
            on screen: NSScreen
        ) {
            let room = screen.visibleFrame
            if size == frame.size, room.intersects(frame) {
                window.setFrame(
                    window.constrainFrameRect(frame, to: screen),
                    display: false
                )
                return
            }
            // Keep the top left in place when the content changes size,
            // moving the frame inward if the new size needs it.
            let fitted = fitting(size, in: room)
            let x = min(max(frame.minX, room.minX), room.maxX - fitted.width)
            let top = min(max(frame.maxY, room.minY + fitted.height), room.maxY)
            window.setFrame(
                CGRect(
                    x: x,
                    y: top - fitted.height,
                    width: fitted.width,
                    height: fitted.height
                ),
                display: false
            )
        }

        @MainActor
        private static func center(
            _ window: NSWindow,
            size: CGSize,
            on screen: NSScreen
        ) {
            let room = screen.visibleFrame
            let fitted = fitting(size, in: room)
            window.setFrame(
                CGRect(
                    x: room.midX - fitted.width / 2,
                    y: room.midY - fitted.height / 2,
                    width: fitted.width,
                    height: fitted.height
                ),
                display: false
            )
        }

        private static func fitting(_ size: CGSize, in room: CGRect) -> CGSize {
            CGSize(
                width: min(size.width, room.width),
                height: min(size.height, room.height)
            )
        }
    }
}
