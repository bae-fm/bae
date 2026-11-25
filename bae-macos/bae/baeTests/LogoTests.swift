import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct LogoTests {
    @Test
    func aFixedClockKeepsTheBobbingLogoAtRest() async throws {
        let still = try await wordBounds(bob: false)
        let bobbing = try await wordBounds(bob: true)
        #expect(bobbing == still)
    }

    /// The word is a whole number of points wide, as the board and the
    /// Android app draw it: 0.52 of the sheep's 120 is 62.4, drawn 62. Its
    /// height is not checked: the word sits under the sheep's 111.99 points,
    /// so the accessibility frame rounds its edges out to the pixel grid.
    @Test
    func theWordIsWholePointsWide() async throws {
        let word = try await wordBounds(bob: false)
        #expect(word.width == 62)
    }

    private func wordBounds(bob: Bool) async throws -> CGRect {
        let size = CGSize(width: 280, height: 180)
        let view = Logo(size: 120, bob: bob, glow: true)
            .frame(width: size.width, height: size.height)
            .environment(\.clockNow, Date(timeIntervalSinceReferenceDate: 0))
            .transaction {
                $0.animation = nil
                $0.disablesAnimations = true
            }
        return try await SnapshotTestSupport.withHostedWindow(view, size: size)
        {
            window,
            host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "the logo"
            )
            let word = try #require(
                tree.nodes.first {
                    $0.role == NSAccessibility.Role.image.rawValue
                        && ($0.description ?? $0.title) == "bae"
                }
            )
            return word.frame
        }
    }
}
