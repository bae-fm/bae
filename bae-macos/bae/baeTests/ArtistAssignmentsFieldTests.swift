import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Artist assignments field")
struct ArtistAssignmentsFieldTests {
    /// The closed field badges a picked library artist and a new one
    /// differently, from the summary it draws.
    @MainActor
    @Test("library and new artists remain visibly distinct")
    func assignmentIdentityIsVisible() throws {
        let picked = PreviewData.pickedArtist(
            "Artist Name",
            artistId: "artist-1"
        )
        let credited = PreviewData.artistCredit("Artist Name")
        let resolutions = [PreviewData.resolvedCredit("Artist Name", .new)]

        let linked = try #require(
            ArtistAssignmentsSummary(
                assignments: [picked],
                resolutions: resolutions
            )?
            .identityLabel
        )
        let new = try #require(
            ArtistAssignmentsSummary(
                assignments: [credited],
                resolutions: resolutions
            )?
            .identityLabel
        )
        #expect(linked != new, "both read \(linked)")
    }

    /// A compilation credits more artists than the line can hold. The header
    /// summarizes them into the width it has instead of running off the pane,
    /// and the album's year keeps a line of its own under them, starting where
    /// the title does.
    @MainActor
    @Test("a compilation's artists stay inside the header")
    func manyArtistsStayInsideTheHeader() async throws {
        let size = NSSize(width: 900, height: 400)
        let draft = PreviewData.manyAlbumArtistsDraft()
        try await withHostedHeader(values: draft, size: size) { _, host in

            try await SnapshotTestSupport.settle(host)
            let frames = SnapshotTestSupport.descendants(of: host)
                .map { $0.convert($0.bounds, to: host) }
            let pane = host.bounds.insetBy(dx: -0.5, dy: -0.5)
            for frame in frames {
                #expect(pane.contains(frame), "\(frame) leaves \(host.bounds)")
            }

            let yearFrame = try #require(
                frame(ofFieldShowing: draft.albumYear, in: host)
            )
            let titleFrame = try #require(
                frame(ofFieldShowing: draft.albumTitle, in: host)
            )
            let otherFields = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .filter { $0.stringValue != draft.albumYear }
                .map { $0.convert($0.bounds, to: host) }
            #expect(
                !otherFields.contains {
                    $0.midY > yearFrame.minY && $0.midY < yearFrame.maxY
                },
                "the album year shares its line with no other field"
            )
            #expect(abs(yearFrame.minX - titleFrame.minX) < 1)
        }
    }

    @MainActor
    private func withHostedHeader<Value>(
        values: BridgeRawReleaseEdit,
        size: NSSize,
        _ body: (NSWindow, NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseMetadataHeader(
                values: values,
                writer: ReleaseFieldWriter(setField: { _, _ in }),
                editingCommands: EditingCommitCommands(),
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) },
                cover: { Color.clear },
                audioFacts: { EmptyView() }
            )
            .padding(24)
            .frame(width: size.width, height: size.height)
            .environment(UiStore()),
            size: size
        ) {
            try await body($0, $1)
        }
    }

    /// Where the field showing `value` sits in `host`.
    @MainActor
    private func frame(
        ofFieldShowing value: String,
        in host: NSView
    ) -> CGRect? {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSTextField }
            .first { $0.stringValue == value }
            .map { $0.convert($0.bounds, to: host) }
    }
}
