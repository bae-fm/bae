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
}
