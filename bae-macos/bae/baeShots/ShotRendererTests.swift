import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// How the renderer draws a scene shown in its window, whatever displays the
/// Mac has.
@MainActor
struct ShotRendererTests {
    private let display: ShotDisplay

    init() async throws {
        display = try await ShotDisplay.shared()
    }

    /// A scene shown in a window 100 points wider than the shot display.
    private static func tooWide(
        _ view: some View = Color.clear
    ) -> ShotScene {
        ShotScene(
            id: "a window too wide for the shot display",
            size: CGSize(width: ShotDisplay.size.width + 100, height: 300),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(view)
        }
    }

    /// A scene shown in a window at its full size: the library window with
    /// an alert over it, 1200 by 1000 points, taller than the free area of a
    /// 1728 by 1117 laptop display and wider than a small display's.
    @Test
    func aWindowLargerThanTheMacsDisplayRendersWhole() async throws {
        let scene = try #require(
            ShotScene.all.first { $0.id == "mac.album.delete-album" }
        )
        let capture = try await ShotRenderer.render(scene, on: display)
        let image = try #require(NSBitmapImageRep(data: capture.png))
        #expect(image.pixelsWide == 2_400)
        #expect(image.pixelsHigh == 2_000)
        #expect(capture.elements.size.width == 1_200)
        #expect(capture.elements.size.height == 1_000)
        #expect(
            capture.elements.elements.contains { $0.label == "Delete album" }
        )
    }

    /// AppKit would shrink the window to fit the shot display; the render
    /// fails instead of capturing the scene narrower than it is.
    @Test
    func aWindowWiderThanTheShotDisplayFails() async throws {
        let error = await #expect(throws: ShotError.self) {
            try await ShotRenderer.render(Self.tooWide(), on: display)
        }
        guard case .windowTooLarge = error else {
            Issue.record(
                "expected the window to be too large: \(String(describing: error))"
            )
            return
        }
    }

    /// A scene that fails to place its window takes down the window and the
    /// alert its view raised: the next scene lists only its own alert.
    @Test
    func aWindowThatFailsToPlaceLeavesNoAlertUp() async throws {
        let tooWide = Self.tooWide(
            OverWindow.alert(
                DeleteFormatConfirmation.alert(named: "MP3") {},
                over: Color.clear
            )
        )
        await #expect(throws: ShotError.self) {
            try await ShotRenderer.render(tooWide, on: display)
        }

        let next = try #require(
            ShotScene.all.first { $0.id == "mac.welcome.remove.failed" }
        )
        let list = try await ShotRenderer.render(next, on: display).elements
        let json = try #require(String(bytes: try list.json(), encoding: .utf8))
        #expect(
            !list.elements.contains { $0.label == "Delete the MP3 format?" },
            "\(json)"
        )
    }
}
