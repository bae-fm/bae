import AppKit
import BaeKit
import ScreenCaptureKit

extension ShotRenderer {
    /// Put `window` wholly on `display`, at the top left of its free area:
    /// the screen capture takes only what is on a display, and would cut off
    /// or refuse a window past a display's edge, or a title bar under the
    /// menu bar.
    ///
    /// AppKit can shrink a titled window that is larger than its display
    /// as it sizes it, so a window whose content is no longer the
    /// scene's size did not fit either, and its capture would draw the scene
    /// at a size its board never has.
    static func place(
        _ window: NSWindow,
        on display: ShotDisplay,
        scene: ShotScene
    ) throws {
        let room = try display.screen().visibleFrame
        guard let shown = window.contentView?.frame.size else {
            throw ShotError.noContentView(scene.id)
        }
        guard
            shown == scene.size,
            window.frame.width <= room.width,
            window.frame.height <= room.height
        else {
            throw ShotError.windowTooLarge(
                scene.id,
                content: scene.size,
                window: window.frame.size,
                shown: shown,
                room: room.size
            )
        }
        window.setFrameOrigin(
            NSPoint(x: room.minX, y: room.maxY - window.frame.height)
        )
    }

    /// Wait for `window`, or an alert over it, to be the key window, asking
    /// for that again whenever it loses it: the window server draws a title
    /// bar and toolbar as inactive otherwise, which the app never shows in
    /// use. A scene that takes the focus also waits for the app to be active
    /// and activates it again when it loses that. Any other scene's window
    /// is a non-activating panel, key while another app holds the focus; it
    /// cannot show an alert, whose panel only an active app makes key.
    /// Fails, naming the front app, when the window never becomes key.
    static func becomeKey(
        _ window: NSWindow,
        scene: ShotScene
    ) async throws {
        if !scene.takesFocus, !AlertWindows.shared.panels.isEmpty {
            throw ShotError.alertWithoutFocus(scene.id)
        }
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
                // as active under it: the buttons of an alert whose panel is
                // not key draw as inactive.
                let keyed = AlertWindows.shared.panels.last ?? window
                if scene.takesFocus {
                    if NSApp.isActive, keyed.isKeyWindow { return true }
                    NSApp.activate(ignoringOtherApps: true)
                }
                else if keyed.isKeyWindow {
                    return true
                }
                keyed.makeKeyAndOrderFront(nil)
                return false
            }
        }
        catch is Wait.TimedOut {
            throw ShotError.windowNotKey(scene.id, focus: .now())
        }
        // Draw the frame as active and hand it to the window server.
        window.displayIfNeeded()
        CATransaction.flush()
    }

    /// The alert panels the app shows, once each is drawn, as the app puts an
    /// alert up: a clear panel of its own over the window, which the window
    /// server holds apart from the window it sits on.
    static func alertPanels() async throws -> [CGWindowID] {
        // The app shows an alert when the alert's view appears, a few run
        // loop passes after AppKit orders the window in.
        try await Wait.until {
            AlertWindows.shared.panels.allSatisfy { $0.isVisible }
        }
        return AlertWindows.shared.panels.map { CGWindowID($0.windowNumber) }
    }

    /// Wait for the window server to show each of `windows` on the screen,
    /// which it tells AppKit through each window's occlusion state: the
    /// window server lists a window it has not shown yet with an empty frame,
    /// and ScreenCaptureKit refuses a capture sized from that frame.
    static func shownOnScreen(
        _ windows: [NSWindow],
        scene: ShotScene
    ) async throws {
        let changes = windows.map {
            Wait.Notice(NSWindow.didChangeOcclusionStateNotification, of: $0)
        }
        do {
            try await Wait.until(notices: changes) {
                windows.allSatisfy { $0.occlusionState.contains(.visible) }
            }
        }
        catch is Wait.TimedOut {
            let hidden = windows.first {
                !$0.occlusionState.contains(.visible)
            }
            throw ShotError.windowNotDrawn(scene.id, hidden?.frame ?? .zero)
        }
    }

    /// The window and its alerts as the window server composes them on the
    /// screen, as PNG bytes: a window's toolbar draws its controls on glass,
    /// which a view's own drawing leaves out, and an alert is a window of its
    /// own, so the capture takes what the screen shows over the window's
    /// frame. The window server must already show every one of the windows
    /// on the screen, so it lists each with its frame. A process may capture
    /// its own windows without the screen recording permission.
    /// ScreenCaptureKit's results aren't `Sendable`, so the capture runs and
    /// encodes off the main actor and hands back only the PNG bytes.
    @concurrent
    nonisolated static func windowPNG(
        _ windowID: CGWindowID,
        alerts: [CGWindowID],
        on displayID: CGDirectDisplayID,
        sceneId: String
    ) async throws -> Data {
        let content = try await SCShareableContent.currentProcess
        let held = ([windowID] + alerts)
            .compactMap { id in
                content.windows.first { $0.windowID == id }
            }
        guard
            let parent = held.first(where: { $0.windowID == windowID }),
            held.count == alerts.count + 1
        else {
            throw ShotError.windowNotShared(sceneId)
        }
        guard held.allSatisfy({ !$0.frame.isEmpty && $0.isOnScreen }) else {
            throw ShotError.windowNotDrawn(sceneId, parent.frame)
        }
        guard
            let display = content.displays.first(where: {
                $0.displayID == displayID
            })
        else { throw ShotError.noScreen(sceneId) }
        let filter = SCContentFilter(display: display, including: held)
        let configuration = SCStreamConfiguration()
        configuration.sourceRect = CGRect(
            x: parent.frame.minX - display.frame.minX,
            y: parent.frame.minY - display.frame.minY,
            width: parent.frame.width,
            height: parent.frame.height
        )
        configuration.width = Int(parent.frame.width) * scale
        configuration.height = Int(parent.frame.height) * scale
        configuration.showsCursor = false
        let image = try await SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        )
        let scaled = try ShotImage.scaled(
            image,
            width: configuration.width,
            height: configuration.height,
            sceneId: sceneId
        )
        let rep = NSBitmapImageRep(cgImage: scaled)
        rep.size = parent.frame.size
        guard let data = rep.representation(using: .png, properties: [:])
        else {
            throw ShotError.pngEncodingFailed(sceneId)
        }
        return data
    }
}
