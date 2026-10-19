import AppKit
import SwiftUI
import Testing

@testable import bae

/// The primary window's size limits while a library is open, and what a
/// change in the library's content costs the window that hosts it.
@MainActor
@Suite("MainWindowChrome")
struct MainWindowChromeTests {
    @Test("the window keeps the library's minimum size and has no maximum")
    func windowSizeLimits() async throws {
        let shown = ShownLine()
        let sizings = ContentSizings()
        try await SnapshotTestSupport.withHostedWindow(
            MeasuredContentHarness(shown: shown, sizings: sizings),
            size: Self.windowSize
        ) { window, host in
            host.layoutSubtreeIfNeeded()

            let unbounded = CGFloat.greatestFiniteMagnitude
            #expect(window.contentMinSize == MainWindow.minSize)
            #expect(window.contentMaxSize.width >= unbounded)
            #expect(window.contentMaxSize.height >= unbounded)
        }
    }

    @Test("content that changes is sized only at the window's size")
    func contentChangeMeasuresNothingElse() async throws {
        let shown = ShownLine()
        let sizings = ContentSizings()
        try await SnapshotTestSupport.withHostedWindow(
            MeasuredContentHarness(shown: shown, sizings: sizings),
            size: Self.windowSize
        ) { _, host in
            host.layoutSubtreeIfNeeded()
            let placedBefore = sizings.placements

            sizings.proposals = []
            shown.text = "A line long enough to change how wide the content is"
            try await Wait.until { sizings.placements > placedBefore }
            host.layoutSubtreeIfNeeded()

            #expect(
                sizings.proposals.allSatisfy {
                    $0 == ProposedViewSize(Self.windowSize)
                }
            )
        }
    }

    private static let windowSize = CGSize(width: 1_100, height: 700)
}

/// The one line the hosted content shows, which a test changes.
@Observable
@MainActor
private final class ShownLine {
    var text = "A line"
}

/// Every size the hosted content was asked for, and how many times it placed
/// what it shows.
@MainActor
private final class ContentSizings {
    var proposals: [ProposedViewSize] = []
    var placements = 0
}

/// Content that records each size it is asked for and each placement.
private struct RecordingLayout: Layout {
    let sizings: ContentSizings

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache _: inout Void
    ) -> CGSize {
        MainActor.assumeIsolated { sizings.proposals.append(proposal) }
        return subviews.first?.sizeThatFits(proposal) ?? .zero
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache _: inout Void
    ) {
        MainActor.assumeIsolated { sizings.placements += 1 }
        for subview in subviews {
            subview.place(at: bounds.origin, proposal: proposal)
        }
    }
}

private struct MeasuredContentHarness: View {
    let shown: ShownLine
    let sizings: ContentSizings

    var body: some View {
        MainWindowChrome(loadError: nil) {
            RecordingLayout(sizings: sizings) {
                Text(verbatim: shown.text)
            }
        }
    }
}
