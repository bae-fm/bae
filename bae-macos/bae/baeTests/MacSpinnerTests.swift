import AppKit
import SwiftUI
import Testing

@testable import BaeKit

@MainActor
struct MacSpinnerTests {
    @Test(arguments: [false, true])
    func startsWithTheHostedViewsAnimationPolicy(disabled: Bool) async throws {
        let spinner = MacSpinner("Reading audio…")
            .transaction { $0.disablesAnimations = disabled }
        try await SnapshotTestSupport.withHostedWindow(
            spinner,
            size: NSSize(width: 220, height: 40)
        ) { _, host in
            let indicator = try #require(
                host.firstDescendant(ofType: MacSpinner.NativeIndicator.self)
            )
            #expect(indicator.spinning == !disabled)
            #expect(indicator.isDisplayedWhenStopped)
        }
    }
}
