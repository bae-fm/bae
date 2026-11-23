import AppKit
import BaeKit
import ColorSync
import os.log

private let frameLogger = Logger.bae("WindowFrameAutosave")

/// AppKit can restore a frame on the primary display even while its original
/// display is connected. Save the exact placement relative to that display
/// and let AppKit fit the saved frame when the display is gone.
@MainActor
struct WindowFrameAutosave {
    let name: NSWindow.FrameAutosaveName
    let firstScreen: NSScreen?
    let defaults: UserDefaults

    private var key: String { "bae.window-placement.\(name)" }

    func place(_ window: NSWindow, contentSized: Bool) {
        let initialSize = window.frame.size
        let screens = NSScreen.screens
        let registered = window.setFrameAutosaveName(name)
        if !registered {
            let operation = "Register a window frame autosave name"
            let detail = "AppKit rejected the name \(name)"
            frameLogger.error("\(operation): \(detail)")
            CoreBugReporter().report(operation: operation, detail: detail)
        }

        if let saved = readPlacement() {
            if let screen = screens.first(where: {
                Self.displayID($0) == saved.display
            }) {
                let frame = saved.frame.offsetBy(
                    dx: screen.frame.minX,
                    dy: screen.frame.minY
                )
                let size = contentSized ? initialSize : frame.size
                restore(window, frame: frame, size: size, on: screen)
                return
            }
            frameLogger.debug("The saved display for \(name) is disconnected")
            if registered, let screen = window.screen {
                let frame = window.frame
                let size = contentSized ? initialSize : frame.size
                restore(window, frame: frame, size: size, on: screen)
                return
            }
        }

        if let screen = firstScreen ?? screens.first {
            let room = screen.visibleFrame
            let width = min(initialSize.width, room.width)
            let height = min(initialSize.height, room.height)
            window.setFrame(
                CGRect(
                    x: room.midX - width / 2,
                    y: room.midY - height / 2,
                    width: width,
                    height: height
                ),
                display: false
            )
        }
    }

    func save(_ window: NSWindow) {
        guard let screen = window.screen, let display = Self.displayID(screen)
        else {
            frameLogger.debug("Cannot save \(name) without a display")
            return
        }
        let relativeFrame = window.frame.offsetBy(
            dx: -screen.frame.minX,
            dy: -screen.frame.minY
        )
        defaults.set(
            ["display": display, "frame": NSStringFromRect(relativeFrame)],
            forKey: key
        )
    }

    private func readPlacement() -> (display: String, frame: CGRect)? {
        guard let saved = defaults.object(forKey: key) else { return nil }
        if let fields = saved as? [String: String],
            let display = fields["display"], let encoded = fields["frame"]
        {
            let frame = NSRectFromString(encoded)
            if [frame.minX, frame.minY, frame.width, frame.height]
                .allSatisfy(\.isFinite), !frame.isEmpty
            {
                return (display, frame)
            }
        }
        frameLogger.warning(
            "Cannot read the saved placement for \(name): \(saved)"
        )
        return nil
    }

    private func restore(
        _ window: NSWindow,
        frame: CGRect,
        size: CGSize,
        on screen: NSScreen
    ) {
        // A content-sized window can show different content on reopen. Keep
        // its top left in the same place as the new content grows or shrinks.
        var restored = CGRect(
            x: frame.minX,
            y: frame.maxY - size.height,
            width: size.width,
            height: size.height
        )
        if size != frame.size {
            let room = screen.visibleFrame
            restored.size.width = min(size.width, room.width)
            restored.size.height = min(size.height, room.height)
            restored.origin.x = min(
                max(restored.minX, room.minX),
                room.maxX - restored.width
            )
            restored.origin.y = min(
                max(frame.maxY - restored.height, room.minY),
                room.maxY - restored.height
            )
        }
        window.setFrame(
            window.constrainFrameRect(restored, to: screen),
            display: false
        )
    }

    private static func displayID(_ screen: NSScreen) -> String? {
        let key = NSDeviceDescriptionKey("NSScreenNumber")
        guard let id = screen.deviceDescription[key] as? CGDirectDisplayID,
            let uuid = CGDisplayCreateUUIDFromDisplayID(id)?.takeRetainedValue()
        else { return nil }
        return CFUUIDCreateString(nil, uuid) as String
    }
}
