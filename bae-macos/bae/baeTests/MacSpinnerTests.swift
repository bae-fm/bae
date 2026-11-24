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
}
