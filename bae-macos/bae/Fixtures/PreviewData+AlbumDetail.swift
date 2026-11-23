import BaeKit
import SwiftUI

@testable import bae

// Fixtures for the album detail's shots and tests: a shot album as its
// detail read returns it, its primary release, one track, and a release in a
// chosen storage state.

extension PreviewData {
    /// One shot album as its detail read returns it. Fails loud when
    /// the fixture id is unseeded.
    @MainActor
    static func albumDetail(albumId: String) -> AlbumDetail {
        guard let bridge = albumDetails[albumId] else {
            fatalError("no preview album for id: \(albumId)")
        }
        return AlbumDetail(bridge: bridge)
    }

    /// The primary release's fat detail for a seeded shot album — tracks,
    /// groups, and duration, drawn from the same seed the album-detail tree
    /// reads. Used by the leaf shots that take a `ReleaseDetail` directly
    /// (track list, track row).
    @MainActor
    static func releaseDetail(albumId: String) -> ReleaseDetail {
        albumDetail(albumId: albumId).shownRelease
    }

    /// One `Track` for the `TrackRowView` shots. `displayArtist` non-nil
    /// exercises the compilation row (core sets it only for compilations).
    static func previewTrack(
        id: String = "t-preview",
        title: String = "Track Title",
        position: String = "1",
        durationMs: UInt64 = 214_000,
        displayArtist: String? = nil
    ) -> Track {
        Track(
            from: previewBridgeTrack(
                id: id,
                title: title,
                position: position,
                durationMs: durationMs,
                artist: displayArtist ?? "Artist Name",
                displayArtist: displayArtist
            )
        )
    }

    /// One `BridgeTrack`, by `artist` on its line.
    static func previewBridgeTrack(
        id: String,
        title: String,
        position: String,
        durationMs: UInt64,
        artist: String,
        displayArtist: String? = nil
    ) -> BridgeTrack {
        BridgeTrack(
            id: id,
            title: title,
            durationMs: durationMs,
            durationClock: BridgeDurationClock.length(ms: durationMs),
            artistLine: .named(artist),
            displayArtist: displayArtist.map(BridgeCreditLine.named),
            positionText: position,
            works: [],
            rating: .stars(mine: nil)
        )
    }

    /// A release in a chosen storage state, as the release's read gives it,
    /// for the storage band and storage-sheet shots: the storage status
    /// line, the pin flag and the actions core computes all vary by
    /// locality, so each state is its own fixture.
    static func storageBridgeRelease(
        storageState: BridgeReleaseStorageState,
        storageActions: [BridgeReleaseStorageAction],
        playability: BridgeReleasePlayability
    ) -> BridgeRelease {
        let files = previewReleaseFiles
        let facts = PreviewData.pressingFacts(
            country: "US",
            media: PreviewData.media(.cd)
        )
        let bridge = BridgeRelease(
            id: "rel-storage-preview",
            albumId: "a-storage-preview",
            name: .described(
                year: 2019,
                media: bridgeMediaTerms(media: PreviewData.media(.cd))
            ),
            distinctYear: nil,
            labels: [
                BridgeLabelLine(
                    names: ["Some Label"],
                    catalogNumbers: ["CAT-0001"]
                )
            ],
            facts: facts,
            areaTerms: bridgeAreaTerms(facts: facts),
            mediaTerms: bridgeMediaTerms(media: facts.media),
            pressingDetails: bridgePressingDetails(facts: facts),
            storageState: storageState,
            storageActions: storageActions,
            upload: nil,
            transitioning: false,
            tracks: [],
            trackGroups: [],
            credits: [],
            hasCredits: false,
            performsWorks: false,
            sourceAudio: .uniform(
                descriptor: BridgeSourceAudioDescriptor(
                    layout: .file,
                    format: previewReleaseAudioFormat
                )
            ),
            otherFiles: [],
            hasOtherFiles: false,
            imageFiles: [previewReleaseCoverFile],
            galleryItems: [],
            galleryItemCount: 0,
            playability: playability,
            offersPlayback: playability == .playable,
            showsTransferLines: false,
            moveToThisDevice: nil,
            downloadStatus: nil,
            // Every catalog, so the expansion's facts-line popover shows
            // what a release a MusicBrainz pick linked all the way out
            // reads like.
            records: PreviewData.releaseRecordsEveryCatalog,
            hasRecords: true,
            totalDuration: .minutesOnly(minutes: 39),
            fileCount: Int64(files.count),
            totalSize: files.reduce(UInt64(0)) { $0 + $1.fileSize },
            cover: nil
        )
        return bridge
    }
}
