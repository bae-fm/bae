import BaeKit
import SwiftUI

@testable import bae

// Fixture support for `AlbumDetailView` and `AlbumExpansionContent`.
// Answers the detail view's album read from the shot albums, injects
// the services the view tree pulls from the environment, and factors the
// `AlbumExpansionContent` no-op-callback boilerplate shared by its
// shots into one builder.

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

    /// Builds an `AlbumExpansionContent` over a shot album with no-op
    /// callbacks — the shared body of `AlbumExpansionContent`'s shots.
    @MainActor
    static func albumExpansionContent(
        detail: AlbumDetail,
        currentTrackId: String? = nil,
        isPlaying: Bool = false,
    ) -> some View {
        AlbumExpansionContent(
            detail: detail,
            onBrowseImages: {},
            onSelectRelease: { _ in },
            currentTrackId: currentTrackId,
            isPlaying: isPlaying,
            onClose: {},
            onPlay: {},
            onShuffle: {},
            onPlayFromTrack: { _ in },
            onTogglePlayPause: {},
            onAddNext: { _ in },
            onAddToQueue: { _ in },
            onAddNextAlbum: {},
            onAddAlbumToQueue: {},
            onEdit: {},
            onOpenStorage: {},
            onExportRelease: {},
            onSaveReleaseAs: {},
            onSetPrimaryRelease: {},
            onDeleteRelease: {},
            onDeleteAlbum: {},
            onExportTrack: { _ in },
            onOpenWork: { _ in },
            onRateAlbum: { _ in },
            onRateTrack: { _, _ in },
        )
    }

    /// The expanded album detail for one seeded fixture album, sized to its
    /// natural width over the dark background — the shared body of
    /// `AlbumExpansionContent`'s shots and the `album-detail` gallery
    /// scene. Builds the content over the album's primary release.
    @MainActor
    static func albumExpansionScene(
        albumId: String,
        currentTrackId: String? = nil,
        isPlaying: Bool = false
    ) -> some View {
        albumExpansionScene(
            detail: albumDetail(albumId: albumId),
            currentTrackId: currentTrackId,
            isPlaying: isPlaying
        )
    }

    /// The expanded album detail for one seeded fixture album whose
    /// primary release reads as `primaryPlayability`, for the line a
    /// release this Mac can't play shows under its facts.
    @MainActor
    static func albumExpansionScene(
        albumId: String,
        primaryPlayability: BridgeReleasePlayability
    ) -> some View {
        guard var bridge = albumDetails[albumId],
            let primary = bridge.releases.firstIndex(where: {
                $0.id == bridge.album.primaryReleaseId
            })
        else {
            fatalError("no preview album for id: \(albumId)")
        }
        bridge.releases[primary].playability = primaryPlayability
        return albumExpansionScene(
            detail: AlbumDetail(bridge: bridge.showing(chosenReleaseId: nil)),
            currentTrackId: nil,
            isPlaying: false
        )
    }

    @MainActor
    private static func albumExpansionScene(
        detail: AlbumDetail,
        currentTrackId: String?,
        isPlaying: Bool
    ) -> some View {
        albumExpansionContent(
            detail: detail,
            currentTrackId: currentTrackId,
            isPlaying: isPlaying
        )
        .padding()
        .frame(width: 1100)
        .background(Theme.background)
        .environment(UiStore())
        .environment(ImageStore.stub())
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
            from: BridgeTrack(
                id: id,
                title: title,
                durationMs: durationMs,
                durationClock: bridgeLengthClock(ms: durationMs),
                artistLine: .named(displayArtist ?? "Artist Name"),
                displayArtist: displayArtist.map(BridgeCreditLine.named),
                positionText: position,
                works: [],
                myStars: nil,
            )
        )
    }

    /// A `ReleaseDetail` in a chosen storage state, for the storage band and
    /// storage-sheet shots: the storage status line, the pin flag, and the
    /// core-computed actions all vary by locality, so each state is its own
    /// fixture rather than a mutation of one.
    @MainActor
    static func storageRelease(
        storageState: BridgeReleaseStorageState,
        storageActions: [BridgeReleaseStorageAction],
        playability: BridgeReleasePlayability
    ) -> ReleaseDetail {
        ReleaseDetail(
            bridge: storageBridgeRelease(
                storageState: storageState,
                storageActions: storageActions,
                playability: playability
            )
        )
    }

    /// The release `storageRelease` shows, as the release's read gives it.
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
            pressingSummary: bridgePressingSummary(facts: facts),
            mediaTerms: bridgeMediaTerms(media: facts.media),
            pressingDetails: bridgePressingDetails(facts: facts),
            storageState: storageState,
            storageActions: storageActions,
            upload: nil,
            transitioning: false,
            tracks: [],
            trackGroups: [],
            credits: [],
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
            coverFiles: [previewReleaseCoverFile],
            galleryItems: [],
            playability: playability,
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
