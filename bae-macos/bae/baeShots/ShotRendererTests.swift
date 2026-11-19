import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// How the renderer treats a scene shown in its window that no display has
/// room for.
@MainActor
struct ShotRendererTests {
    /// A scene shown in a window 100 points wider than every display.
    private static func tooWide(
        _ view: some View = Color.clear
    ) -> ShotScene {
        let widest = NSScreen.screens.map(\.frame.width).max() ?? 0
        return ShotScene(
            id: "a window too wide for every display",
            size: CGSize(width: widest + 100, height: 300),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(view)
        }
    }

    /// AppKit shrinks the window to the display's width; the render fails
    /// instead of capturing the scene narrower than it is.
    @Test
    func aWindowWiderThanEveryDisplayFails() async throws {
        let error = await #expect(throws: ShotError.self) {
            try await ShotRenderer.render(Self.tooWide())
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
            try await ShotRenderer.render(tooWide)
        }

        let next = try #require(
            ShotScene.all.first { $0.id == "mac.welcome.remove.failed" }
        )
        let list = try await ShotRenderer.render(next).elements
        let json = try #require(String(bytes: try list.json(), encoding: .utf8))
        #expect(
            !list.elements.contains { $0.label == "Delete the MP3 format?" },
            "\(json)"
        )
    }
}
