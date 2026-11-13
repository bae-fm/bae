import AppKit
import BaeKit
import ScreenCaptureKit

extension ShotRenderer {
    /// The alert panels showing over `window`, once each is drawn, as the app
    /// puts an alert up: a clear panel of its own over the window, which the
    /// window server holds apart from the window it sits on.
    static func sheets(
        of window: NSWindow,
        scene: ShotScene
    ) async throws -> [CGWindowID] {
        // An alert is raised as its view appears, a few turns after the
        // window is ordered in.
        try await Wait.until {
            AlertWindows.shared.panels.allSatisfy { $0.isVisible }
        }
        return AlertWindows.shared.panels.map { CGWindowID($0.windowNumber) }
    }

    /// The window and its alerts as the window server composes them on the
    /// screen, as PNG bytes: a window's toolbar draws its controls on glass,
    /// which a view's own drawing leaves out, and an alert is a window of its
    /// own, so the capture takes what the screen shows over the window's
    /// frame. A process may capture its own windows without the screen
    /// recording permission. ScreenCaptureKit's results aren't `Sendable`, so
    /// the capture runs and encodes off the main actor and hands back only the
    /// PNG bytes.
    @concurrent
    nonisolated static func windowPNG(
        _ windowID: CGWindowID,
        sheets: [CGWindowID],
        sceneId: String
    ) async throws -> Data {
        // The window server lists a window a moment after it is ordered in,
        // with an empty frame until it is drawn; a capture sized from that
        // frame is refused as an invalid parameter. Ask again until every
        // window has a frame.
        var content: SCShareableContent?
        var parent: SCWindow?
        var held: [SCWindow] = []
        for _ in 0..<50 {
            let listed = try await SCShareableContent.currentProcess
            let found = ([windowID] + sheets)
                .compactMap { id in
                    listed.windows.first { $0.windowID == id }
                }
            content = listed
            parent = found.first { $0.windowID == windowID }
            held = found
            if found.count == sheets.count + 1,
                found.allSatisfy({ !$0.frame.isEmpty && $0.isOnScreen })
            {
                break
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        guard let content, let parent, held.count == sheets.count + 1 else {
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
