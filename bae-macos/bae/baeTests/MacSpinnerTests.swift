import AppKit
import SwiftUI
import Testing

@testable import BaeKit

@MainActor
struct MacSpinnerTests {
    private static func indicator(
        in host: NSView
    ) throws -> MacSpinner.NativeIndicator {
        try #require(
            host.firstDescendant(ofType: MacSpinner.NativeIndicator.self)
        )
    }

    /// The running app's views are created in a transaction that disables
    /// animations, and its spinners turn all the same.
    @Test(arguments: [false, true])
    func turnsInAWindowWhateverTheTransaction(disabled: Bool) async throws {
        let spinner = MacSpinner("Reading audio…")
            .transaction { $0.disablesAnimations = disabled }
        try await SnapshotTestSupport.withHostedWindow(
            spinner,
            size: NSSize(width: 220, height: 40)
        ) { _, host in
            let indicator = try Self.indicator(in: host)
            #expect(indicator.animating)
            #expect(indicator.isDisplayedWhenStopped)
        }
    }

    @Test
    func holdsStillInAWindowThatHoldsSpinnersStill() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            MacSpinner("Reading audio…"),
            size: NSSize(width: 220, height: 40)
        ) { window, host in
            let indicator = try Self.indicator(in: host)
            #expect(indicator.animating)
            window.holdsSpinnersStill = true
            #expect(!indicator.animating)
            window.holdsSpinnersStill = false
            #expect(indicator.animating)
        }
    }

    @Test
    func stopsWhenItLeavesTheWindowAndStartsWhenAWindowTakesItBack()
        async throws
    {
        try await SnapshotTestSupport.withHostedWindow(
            MacSpinner("Reading audio…"),
            size: NSSize(width: 220, height: 40)
        ) { window, host in
            let indicator = try Self.indicator(in: host)
            let parent = try #require(indicator.superview)
            indicator.removeFromSuperview()
            #expect(!indicator.animating)
            parent.addSubview(indicator)
            #expect(indicator.window === window)
            #expect(indicator.animating)
        }
    }

    private static func panelWithSpinner() -> NSPanel {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 160, height: 40),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        panel.contentView = NSHostingView(rootView: MacSpinner())
        return panel
    }

    /// The menus, popovers and alerts of a window are its child windows,
    /// put under it after their content is in them.
    @Test
    func holdsStillInAChildWindowOfAWindowThatHoldsSpinnersStill() async throws
    {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear,
            size: NSSize(width: 220, height: 40)
        ) { window, _ in
            window.holdsSpinnersStill = true
            let panel = Self.panelWithSpinner()
            window.addChildWindow(panel, ordered: .above)
            panel.orderFront(nil)
            defer { panel.close() }
            #expect(panel.holdsSpinnersStill)
            panel.contentView?.layoutSubtreeIfNeeded()
            panel.displayIfNeeded()
            let indicator = try #require(
                panel.contentView?
                    .firstDescendant(
                        ofType: MacSpinner.NativeIndicator.self
                    )
            )
            #expect(!indicator.animating)
        }
    }

    @Test
    func holdsStillInASheetOfAWindowThatHoldsSpinnersStill() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear,
            size: NSSize(width: 220, height: 40)
        ) { window, _ in
            window.holdsSpinnersStill = true
            let sheet = Self.panelWithSpinner()
            window.beginSheet(sheet)
            defer { window.endSheet(sheet) }
            #expect(sheet.holdsSpinnersStill)
        }
    }

    @Test
    func aWindowSetOnItsOwnIgnoresItsParent() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear,
            size: NSSize(width: 220, height: 40)
        ) { window, _ in
            window.holdsSpinnersStill = true
            let panel = Self.panelWithSpinner()
            panel.holdsSpinnersStill = false
            window.addChildWindow(panel, ordered: .above)
            defer { panel.close() }
            #expect(!panel.holdsSpinnersStill)
        }
    }
}
