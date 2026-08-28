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
    @Test("same-name choices expose their exact library identity")
    func sameNameChoicesRemainDistinct() async throws {
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

        #expect(firstImage != secondImage)
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
            .environment(Library.stub()),
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
