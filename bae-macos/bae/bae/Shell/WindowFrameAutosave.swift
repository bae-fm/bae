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
            let target: (frame: CGRect, screen: NSScreen)
            if let screen = screens.first(where: {
                displayID($0) == saved.display
            }) {
                target = (
                    saved.frame.offsetBy(
                        dx: screen.frame.minX,
                        dy: screen.frame.minY
                    ),
                    screen
                )
            }
            else {
                frameLogger.debug(
                    "The saved display for \(autosave.name) is unavailable"
                )
                guard registered, window.setFrameUsingName(autosave.name),
                    let screen = window.screen
                else {
                    frameLogger.warning(
                        "AppKit could not restore a saved frame for \(autosave.name)"
                    )
                    Self.center(
                        window,
                        size: contentFrameSize ?? saved.frame.size,
                        on: firstScreen
                    )
                    return
                }
                target = (window.frame, screen)
            }
            Self.restore(
                window,
                frame: target.frame,
                size: contentFrameSize ?? target.frame.size,
                on: target.screen
            )
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

    private var key: String { "bae.window-placement.\(name)" }

    func save(_ window: NSWindow) {
        guard let screen = window.screen else {
            frameLogger.debug("Cannot save \(name) without a display")
            return
        }
        guard let display = Self.displayID(screen) else {
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
        frameLogger.error("\(operation): \(detail)")
        bugReporter.report(operation: operation, detail: detail)
        showError(
            .Diagnostic(category: .internal, detail: "\(operation): \(detail)")
        )
    }

    private struct SavedFrame {
        /// Coordinates relative to the saved display.
        let frame: CGRect
        let display: UUID
    }

    private func readPlacement() -> SavedFrame? {
        guard let value = defaults.object(forKey: key) else { return nil }
        if let fields = value as? [String: String],
            let id = fields["display"], let display = UUID(uuidString: id),
            let encoded = fields["frame"]
        {
            let frame = NSRectFromString(encoded)
            if [frame.minX, frame.minY, frame.width, frame.height]
                .allSatisfy(\.isFinite), !frame.isEmpty
            {
                return SavedFrame(frame: frame, display: display)
            }
        }
        frameLogger.warning(
            "Cannot read the saved placement for \(name): \(value)"
        )
        return nil
    }

    private static func displayID(_ screen: NSScreen) -> UUID? {
        guard let id = screen.displayNumber,
            let uuid = CGDisplayCreateUUIDFromDisplayID(id)?.takeRetainedValue()
        else { return nil }
        return UUID(uuidString: CFUUIDCreateString(nil, uuid) as String)
    }
}
