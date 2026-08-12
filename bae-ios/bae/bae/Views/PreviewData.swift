#if DEBUG
import BaeKit
import SwiftUI

/// Fixture data and preview-only environment wiring for the iOS views.
/// Every fixture uses generic placeholders — never a real artist, album, or
/// track name. Screen previews inject empty-but-live stores plus fresh
/// service stubs through `.previewStores()`; leaf previews take fixture values
/// directly.
enum PreviewData {
    // MARK: - Config

    @MainActor
    static func configStore() -> ConfigStore {
        ConfigStore(
            config: Config(
                bridge: BridgeConfig(
                    libraryId: "lib-preview",
                    libraryName: "Preview Library",
                    libraryPath: "/preview",
                    encryptionKeyStored: false,
                    encryptionKeyFingerprint: nil,
                    pauseBetweenSides: false,
                    maxConcurrentUploads: 3,
                    maxConcurrentDownloads: 3,
                    showRemainingTime: false,
                    libraryFullWidth: false,
                    savePresets: [],
                    defaultTrackSavePreset: "flac",
                    defaultReleaseSavePreset: "flac",
                    castEnabled: false,
                    mcp: BridgeMcpConfig(enabled: false, port: 47777),
                    subsonic: BridgeSubsonicConfig(
                        enabled: false,
                        port: 4533,
                        username: "",
                        bindAddress: "127.0.0.1"
                    ),
                    discogsTokenStatus: .notConfigured,
                    discogsUsable: false,
                    sync: nil
                )
            )
        )
    }

    // MARK: - Albums

    static let albums: [BridgeAlbum] = (1...6)
        .map { index in
            BridgeAlbum(
                id: "a-\(index)",
                title: "Album Title \(index)",
                year: Int32(2010 + index),
                isCompilation: false,
                artistNames: "Artist Name \(index)",
                releaseIds: ["rel-a-\(index)"],
                primaryReleaseId: "rel-a-\(index)",
                cover: nil
            )
        }

    static func tracks(count: Int) -> [BridgeTrack] {
        (1...count)
            .map { index in
                // Bound values first: the single-expression form exceeds the
                // type checker's time budget on CI hardware.
                let durationMs = Int64(180_000 + index * 15_000)
                let trackNumber = Int32(index)
                return BridgeTrack(
                    id: "t-\(index)",
                    title: "Track Title \(index)",
                    side: 1,
                    trackNumber: trackNumber,
                    durationMs: durationMs,
                    durationClock: bridgeClock(ms: durationMs),
                    artistNames: "Artist Name 1",
                    displayArtist: nil,
                    positionText: String(index)
                )
            }
    }

    static func release(albumId: String) -> BridgeRelease {
        let songs = tracks(count: 8)
        return BridgeRelease(
            id: "rel-\(albumId)",
            albumId: albumId,
            displayName: "2018 CD",
            year: 2018,
            format: "CD",
            label: "Label Name",
            catalogNumber: "CAT-001",
            country: "US",
            storageState: .local,
            pinned: false,
            storageActions: [],
            transferAction: nil,
            tracks: songs,
            trackGroups: [BridgeTrackGroup(side: .flat, headerKey: nil, tracks: songs)],
            files: [],
            imageFiles: [],
            galleryItems: [],
            totalDuration: .minutesOnly(minutes: 39),
            fileCount: 0,
            totalSize: 0,
            cover: nil
        )
    }

    // MARK: - Browse summaries

    static let composerSummary = BridgeComposerSummary(
        artistId: "composer-1",
        name: "Composer Name 1",
        sortName: "Composer Name 1",
        workCount: 4,
        linkedReleaseCount: 6,
        unlinkedCreditCount: 0,
        image: nil
    )

    static let artistSummary = BridgeArtistSummary(
        artistId: "artist-1",
        name: "Artist Name 1",
        albumCount: 3,
        image: nil
    )

    static let workSummary = BridgeWorkSummary(
        workId: "work-1",
        title: "Work Title 1",
        disambiguation: nil,
        workType: "work",
        parentWorkId: nil,
        composerNames: "Composer Name 1",
        linkedReleaseCount: 2,
        representativeReleaseId: "rel-a-1",
        representativeCover: nil
    )

    // MARK: - Queue / now playing

    static let queueEntries: [BridgeQueueEntry] = (1...5)
        .map { index in
            BridgeQueueEntry(
                entryId: "e-\(index)",
                trackId: "t-\(index)",
                title: "Track Title \(index)",
                artistNames: "Artist Name 1",
                durationClock: bridgeClock(ms: Int64(180_000 + index * 15_000)),
                albumTitle: "Album Title 1",
                coverImage: nil
            )
        }

    static var queueItem: QueueItem {
        QueueItem(bridge: queueEntries[0])
    }

    @MainActor
    static let nowPlayingTrack = NowPlayingTrack(
        trackId: "t-1",
        trackTitle: "Track Title 1",
        artistNames: "Artist Name 1",
        albumId: "a-1",
        coverImage: nil,
        durationMs: 195_000
    )

    // MARK: - Downloads

    static func downloadSnapshot(
        queued: UInt32 = 0,
        active: UInt32 = 0,
        failed: UInt32 = 0,
        paused: Bool = false,
        ops: [BridgeDownloadOp] = []
    ) -> BridgeDownloadSnapshot {
        BridgeDownloadSnapshot(
            downloads: ops,
            total: BridgeDownloadProgress(
                queued: queued,
                active: active,
                failed: failed
            ),
            summaryParts: [],
            paused: paused
        )
    }

    static let queuedDownloadOp = BridgeDownloadOp(
        releaseId: "rel-a-1",
        title: "Album Title 1",
        fileCount: 12,
        totalSize: 480_000_000,
        createdAt: 0,
        state: .queued
    )

    // MARK: - Join flow

    static let joinRequest = BridgeJoinRequest(
        code: "PREVIEW-JOIN-CODE",
        fingerprint: "ab12cd34"
    )

    static let inviteInfo = BridgeInviteCodeInfo(
        libraryId: "lib-preview",
        libraryName: "Preview Library",
        ownerPubkey: "owner-pubkey",
        ownerFingerprint: "ef56ab78",
        cloudProvider: .s3,
        needsOauth: false
    )

    static let cloudProviders: [BridgeCloudProvider] = availableCloudProviders()

    // MARK: - Search

    static let searchResults = SearchResults(
        bridge: BridgeSearchResults(
            albums: [
                BridgeAlbumSearchResult(
                    id: "a-1",
                    title: "Album Title 1",
                    year: 2019,
                    artistName: "Artist Name 1",
                    cover: nil
                )
            ],
            artists: [artistSummary],
            tracks: [
                BridgeTrackSearchResult(
                    id: "t-1",
                    title: "Track Title 1",
                    durationClock: bridgeClock(ms: 195_000),
                    albumId: "a-1",
                    albumTitle: "Album Title 1",
                    artistName: "Artist Name 1",
                    cover: nil
                )
            ],
            composers: [composerSummary],
            works: [workSummary]
        ),
        query: "placeholder"
    )

}

private final class PreviewLibrarySubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    func cancel() {}
}

// MARK: - Seeded stores

extension PreviewData {
    static func library() -> Library {
        Library(
            subscribeAlbumDetail: { albumId, callback in
                let detail = albums.first { $0.id == albumId }
                    .map {
                        BridgeAlbumDetail(
                            album: $0,
                            releases: [release(albumId: albumId)]
                        )
                    }
                callback.onValue(value: detail)
                return PreviewLibrarySubscription()
            }
        )
    }

    /// A `LibraryStore` holding the fixture album summaries and one seeded
    /// release detail, so the grid and album screen render content.
    @MainActor
    static func libraryStore() -> LibraryStore {
        let store = LibraryStore()
        for album in albums {
            _ = store.internAlbumSummary(album)
        }
        _ = store.internReleaseDetail(release(albumId: "a-1"))
        return store
    }

    @MainActor
    static func playbackStore(nowPlaying: Bool = true) -> PlaybackStore {
        let store = PlaybackStore()
        store.applyQueueSnapshot(
            BridgeQueueSnapshot(
                manual: Array(queueEntries.prefix(2)),
                context: BridgePlaybackContext(
                    kind: .release,
                    sourceTitle: "Album Title 1",
                    shuffled: false,
                    upcoming: Array(queueEntries.suffix(from: 2)),
                    upcomingTotal: UInt64(queueEntries.count - 2)
                ),
                hasNext: true,
                hasPrevious: false,
                revision: 1
            )
        )
        if nowPlaying {
            store.nowPlaying = .playing(nowPlayingTrack)
        }
        return store
    }

    @MainActor
    static func downloadStore() -> DownloadStore {
        DownloadStore(snapshot: downloadSnapshot())
    }

    /// The album-detail fixtures, interned through a throwaway store so the
    /// summary and release detail carry the store-side shape the views read.
    @MainActor
    static var albumSummary: AlbumSummary {
        LibraryStore().internAlbumSummary(albums[0])
    }

    @MainActor
    static var releaseDetail: ReleaseDetail {
        LibraryStore().internReleaseDetail(release(albumId: "a-1"))
    }
}

extension View {
    /// Inject empty-but-live stores plus the shared service stubs, so a screen
    /// preview renders its real body against fixture state instead of real data.
    @MainActor
    func previewStores(
        libraryStore: LibraryStore? = nil,
        playbackStore: PlaybackStore? = nil,
        downloadStore: DownloadStore? = nil
    ) -> some View {
        let library = PreviewData.library()
        let resolvedLibraryStore = libraryStore ?? PreviewData.libraryStore()
        return environment(PreviewData.configStore())
            .environment(SyncStatusStore())
            .environment(resolvedLibraryStore)
            .environment(playbackStore ?? PreviewData.playbackStore())
            .environment(downloadStore ?? PreviewData.downloadStore())
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(
                LibraryListsStore(
                    library: library,
                    libraryStore: resolvedLibraryStore,
                    onError: { _ in }
                )
            )
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(Downloads.stub())
            .environment(Sync.stub())
            .environment(ImageStore.stub())
            .environment(Cast.stub())
            .environment(CastStore())
            .environment(RendererBrowser.stub())
    }
}
#endif
