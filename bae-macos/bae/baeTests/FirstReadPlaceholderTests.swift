import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A screen's first read of a local live query lands at once, so nothing
/// stands in for it: no spinner, no line, until its rows arrive. That the
/// pane draws no line is the `artist-pane-first-read` shot's.
@Suite("Local first reads")
struct FirstReadPlaceholderTests {
    @MainActor
    @Test("a selected artist's pane draws no spinner until its read lands")
    func artistPaneDrawsNoSpinnerWhileReading() async throws {
        let uiStore = UiStore()
        let session = PreviewData.browseSession(uiStore: uiStore)
        session.selectArtist("artist-0")
        let size = NSSize(width: 520, height: 640)
        try await SnapshotTestSupport.withHostedWindow(
            ArtistDetailPane(detail: nil)
                .environment(session)
                .environment(uiStore)
                .environment(ArtistEditor())
                .environment(ImageStore.stub()),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty)
        }
    }
}
