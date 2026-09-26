import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Artist assignments field")
struct ArtistAssignmentsFieldTests {
    @MainActor
    @Test("library and new artists remain visibly distinct")
    func assignmentIdentityIsVisible() async throws {
        let picked = PreviewData.pickedArtist(
            "Artist Name",
            artistId: "artist-1"
        )
        let credited = PreviewData.artistCredit("Artist Name")
        let resolutions = [PreviewData.resolvedCredit("Artist Name", .new)]

        let linkedImage = try await render([picked], resolutions: resolutions)
        let newImage = try await render([credited], resolutions: resolutions)

        #expect(linkedImage != newImage)
    }

    @MainActor
    @Test("existing artist results do not present internal IDs")
    func existingArtistResultsHideInternalIds() async throws {
        let first = BridgeExistingArtist(
            artistId: "artist-1",
            name: "Artist Name",
            sortName: "Name, Artist",
            musicbrainzArtistId: nil,
            discogsArtistId: nil
        )
        let second = BridgeExistingArtist(
            artistId: "artist-2",
            name: "Artist Name",
            sortName: "Name, Artist",
            musicbrainzArtistId: nil,
            discogsArtistId: nil
        )

        let firstImage = try await renderChoice(first)
        let secondImage = try await renderChoice(second)

        #expect(firstImage == secondImage)
    }

    /// A compilation credits more artists than the line can hold. The header
    /// summarizes them into the width it has instead of running off the pane,
    /// so the year still follows the artists on that line.
    @MainActor
    @Test("a compilation's artists stay inside the header")
    func manyArtistsStayInsideTheHeader() async throws {
        let size = NSSize(width: 900, height: 360)
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
            let artistField = try #require(
                frames
                    .filter {
                        $0.midY > yearFrame.minY && $0.midY < yearFrame.maxY
                    }
                    .max { $0.width < $1.width }
            )
            #expect(artistField.width > yearFrame.width)
            #expect(artistField.maxX <= yearFrame.minX)
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
                cover: { Color.clear },
                audioFacts: { EmptyView() }
            )
            .padding(24)
            .frame(width: size.width, height: size.height)
            .environment(Library.stub())
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

    @MainActor
    private func render(
        _ assignments: [BridgeArtistAssignment],
        resolutions: [BridgeResolvedCredit]
    ) async throws -> Data {
        let size = NSSize(width: 280, height: 40)
        return try await SnapshotTestSupport.withHostedWindow(
            ArtistAssignmentsField(
                assignments: assignments,
                placeholder: "Artist",
                onChange: { _ in }
            )
            .frame(width: size.width, height: size.height)
            .environment(\.artistResolutions, resolutions)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        ) { _, host in
            return try await SnapshotTestSupport.capturePNG(host, size: size)
        }
    }

    @MainActor
    private func renderChoice(
        _ artist: BridgeExistingArtist
    ) async throws -> Data {
        let size = NSSize(width: 280, height: 48)
        return try await SnapshotTestSupport.withHostedWindow(
            ArtistSearchResultLabel(artist: artist)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            return try await SnapshotTestSupport.capturePNG(host, size: size)
        }
    }
}
