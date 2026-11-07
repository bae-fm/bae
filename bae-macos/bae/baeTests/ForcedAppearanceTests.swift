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
        try await expectSpinnersIn(
            mode: mode,
            onSystem: system,
            content: WaitingIndicator()
        )
    }

    /// A table's cells host their views apart from the window's SwiftUI
    /// tree, so the chosen mode reaches them only through the window.
    @Test(
        "a spinner in a table cell takes the chosen mode, not the system's",
        arguments: [
            (AppearanceMode.light, NSAppearance.Name.darkAqua),
            (AppearanceMode.dark, NSAppearance.Name.aqua),
        ]
    )
    func spinnerInACellTakesTheChosenMode(
        mode: AppearanceMode,
        system: NSAppearance.Name
    ) async throws {
        try await expectSpinnersIn(
            mode: mode,
            onSystem: system,
            content: SpinnerCell()
        )
    }

    /// Host `content` in `mode` as the app hosts a window, the system in
    /// `system`, and expect its one spinner drawn in `mode`.
    private func expectSpinnersIn(
        mode: AppearanceMode,
        onSystem system: NSAppearance.Name,
        content: some View
    ) async throws {
        let systemAppearance = NSApp.appearance
        NSApp.appearance = NSAppearance(named: system)
        defer { NSApp.appearance = systemAppearance }
        let controller = HostingWindowController(
            content:
                content
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
        let root = try #require(window.contentView)

        try await Wait.untilSteady {
            root.layoutSubtreeIfNeeded()
            return Self.spinners(below: root)
                .map { $0.effectiveAppearance.name }
        }

        let spinners = Self.spinners(below: root)
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

/// A spinner in a `HostingTableCell`, drawn as a table's cells draw.
private struct SpinnerCell: NSViewRepresentable {
    func makeNSView(context _: Context) -> HostingTableCell {
        let cell = HostingTableCell()
        cell.host(
            WaitingIndicator()
                .hostedAppearance(
                    HostedAppearance(tone: .standard, accent: .standard)
                )
        )
        return cell
    }

    func updateNSView(_: HostingTableCell, context _: Context) {}
}
