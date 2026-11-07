import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A light or dark mode the person chose over the system's reaches the AppKit
/// controls SwiftUI draws with, in a window the app hosts the way it hosts
/// Settings: a spinner drawn in the system's appearance instead is pale on
/// light, or dark on dark, and barely shows.
@MainActor
@Suite("A chosen light or dark mode reaches AppKit controls")
struct ForcedAppearanceTests {
    @Test(
        "a spinner takes the chosen mode, not the system's",
        arguments: [
            (AppearanceMode.light, NSAppearance.Name.darkAqua),
            (AppearanceMode.dark, NSAppearance.Name.aqua),
        ]
    )
    func spinnerTakesTheChosenMode(
        mode: AppearanceMode,
        system: NSAppearance.Name
    ) async throws {
        let systemAppearance = NSApp.appearance
        NSApp.appearance = NSAppearance(named: system)
        defer { NSApp.appearance = systemAppearance }
        let controller = HostingWindowController(
            content: ProgressView()
                .controlSize(.small)
                .appearance(mode: mode, accent: .standard, tone: .standard),
            contentSize: CGSize(width: 100, height: 60),
            sizing: [],
            bridging: [],
            onClose: {}
        )
        let window = try #require(controller.window)
        // Past every display, so nothing shows on the screen of the person
        // running the suite.
        let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
        window.setFrameOrigin(NSPoint(x: right + 1_000, y: 0))
        window.animationBehavior = .none
        window.orderFront(nil)
        defer { controller.close() }
        let content = try #require(window.contentView)

        try await Wait.untilSteady {
            content.layoutSubtreeIfNeeded()
            return Self.spinners(below: content)
                .map {
                    $0.effectiveAppearance.name
                }
        }

        let spinners = Self.spinners(below: content)
        #expect(spinners.count == 1)
        let chosen: NSAppearance.Name = mode == .light ? .aqua : .darkAqua
        for spinner in spinners {
            #expect(
                spinner.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua])
                    == chosen
            )
        }
    }

    private static func spinners(below view: NSView) -> [NSProgressIndicator] {
        view.subviews.flatMap { subview in
            [subview as? NSProgressIndicator].compactMap { $0 }
                + spinners(below: subview)
        }
    }
}
