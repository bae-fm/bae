import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The chips the filter field shows for the states narrowing the list.
@MainActor
@Suite("The Pending filter chips")
struct PendingFilterChipsTests {
    final class Recorder {
        var cleared: [BridgePendingFilter] = []
    }

    /// A click on the last chip's ✕, which ends the row of chips, clears
    /// that chip's state and no other.
    @Test("a chip's ✕ clears only its own state")
    func aChipClearsOnlyItsOwnState() async throws {
        let recorder = Recorder()
        let chips = PendingFilterChips(
            filters: [.needsYou, .importError],
            onClear: { recorder.cleared.append($0) }
        )
        let width = NSHostingView(rootView: chips).fittingSize.width
        let size = NSSize(width: 480, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            chips.frame(
                width: size.width,
                height: size.height,
                alignment: .leading
            ),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let xmark = ThemeSpace.compact + ThemeIcon.badge.size / 2
            try HostedInput.click(
                at: NSPoint(x: width - xmark, y: size.height / 2),
                in: host
            )
            try await Wait.until { !recorder.cleared.isEmpty }
            #expect(recorder.cleared == [.importError])
        }
    }
}
