import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Artist assignments field")
struct ArtistAssignmentsFieldTests {
    @MainActor
    @Test("linked and new assignments remain visibly distinct")
    func assignmentIdentityIsVisible() async throws {
        let existing = BridgeArtistAssignment.existing(
            artist: BridgeExistingArtist(
                artistId: "artist-1",
                name: "Artist Name",
                sortName: nil,
                musicbrainzArtistId: nil,
                discogsArtistId: nil
            )
        )
        let created = BridgeArtistAssignment.new(
            seed: BridgeNewArtistSeed(
                name: "Artist Name",
                sortName: nil,
                musicbrainzArtistId: nil,
                discogsArtistId: nil
            )
        )

        let linkedImage = try await render([existing])
        let newImage = try await render([created])

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

    @Test("one library artist reads as its own name and badge")
    func oneLibraryArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(assignments: [
                PreviewData.existingArtist("Artist Name", artistId: "artist-1")
            ])
        )

        #expect(summary.names == "Artist Name")
        #expect(summary.identityLabel == "Library")
    }

    @Test("one new artist reads as its own name and badge")
    func oneNewArtistSummary() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(assignments: [
                PreviewData.newArtist("New Artist Name")
            ])
        )

        #expect(summary.names == "New Artist Name")
        #expect(summary.identityLabel == "New")
    }

    @Test("artists all already in the library carry one Library badge")
    func allLibraryArtistsSummary() throws {
        let names = ["First Artist", "Second Artist", "Third Artist"]
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: names.enumerated()
                    .map { index, name in
                        PreviewData.existingArtist(
                            name,
                            artistId: "artist-\(index)"
                        )
                    }
            )
        )

        #expect(
            summary.names == ListFormatter.localizedString(byJoining: names)
        )
        #expect(summary.identityLabel == "Library")
    }

    @Test("artists all new to the library carry one New badge")
    func allNewArtistsSummary() throws {
        let names = ["First Artist", "Second Artist"]
        let summary = try #require(
            ArtistAssignmentsSummary(
                assignments: names.map(PreviewData.newArtist)
            )
        )

        #expect(
            summary.names == ListFormatter.localizedString(byJoining: names)
        )
        #expect(summary.identityLabel == "New")
    }

    @Test("a mixed set counts the artists new to the library")
    func mixedArtistsSummaryCountsTheNewOnes() throws {
        let summary = try #require(
            ArtistAssignmentsSummary(assignments: [
                PreviewData.existingArtist("First Artist", artistId: "a-1"),
                PreviewData.newArtist("Second Artist"),
                PreviewData.existingArtist("Third Artist", artistId: "a-3"),
                PreviewData.newArtist("Fourth Artist"),
            ])
        )

        #expect(
            summary.names
                == ListFormatter.localizedString(
                    byJoining: [
                        "First Artist", "Second Artist", "Third Artist",
                        "Fourth Artist",
                    ]
                )
        )
        #expect(summary.identityLabel == "2 new")
    }

    /// A compilation credits more artists than the line can hold. The header
    /// summarizes them into the width it has instead of running off the pane,
    /// so the year still follows the artists on that line.
    @MainActor
    @Test("a compilation's artists stay inside the header")
    func manyArtistsStayInsideTheHeader() async throws {
        let size = NSSize(width: 900, height: 360)
        let draft = PreviewData.manyAlbumArtistsDraft()
        let (window, host) = hostHeader(values: draft, size: size)
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }

        await SnapshotTestSupport.settle(host)
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
                .filter { $0.midY > yearFrame.minY && $0.midY < yearFrame.maxY }
                .max { $0.width < $1.width }
        )
        #expect(artistField.width > yearFrame.width)
        #expect(artistField.maxX <= yearFrame.minX)
        withExtendedLifetime(window) {}
    }

    @MainActor
    private func hostHeader(
        values: BridgeRawReleaseEdit,
        size: NSSize
    ) -> (window: NSWindow, host: NSView) {
        SnapshotTestSupport.hostInWindow(
            ReleaseMetadataHeader(
                values: values,
                writer: ReleaseFieldWriter(setField: { _, _ in }),
                editingCommands: EditingCommitCommands(),
                cover: { Color.clear },
                context: { EmptyView() },
                sourceAudio: { EmptyView() }
            )
            .padding(24)
            .frame(width: size.width, height: size.height)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        )
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
        _ assignments: [BridgeArtistAssignment]
    ) async throws -> Data {
        let size = NSSize(width: 280, height: 40)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ArtistAssignmentsField(
                assignments: assignments,
                placeholder: "Artist",
                onChange: { _ in }
            )
            .frame(width: size.width, height: size.height)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        return try await SnapshotTestSupport.capturePNG(host, size: size)
    }

    @MainActor
    private func renderChoice(
        _ artist: BridgeExistingArtist
    ) async throws -> Data {
        let size = NSSize(width: 280, height: 48)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ArtistSearchResultLabel(artist: artist)
                .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        return try await SnapshotTestSupport.capturePNG(host, size: size)
    }
}
