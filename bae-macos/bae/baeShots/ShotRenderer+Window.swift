import AppKit
import BaeKit
import ScreenCaptureKit

extension ShotRenderer {
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
                $0.frame.intersects(parent.frame)
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
