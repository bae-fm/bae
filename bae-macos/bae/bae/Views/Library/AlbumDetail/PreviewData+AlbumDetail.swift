#if DEBUG
    import BaeKit
    import SwiftUI

    // Preview support for `AlbumDetailView` and `AlbumExpansionContent`.
    // Seeds a `LibraryStore` with the album summaries and release details the
    // detail view reads, injects the services the view tree pulls from the
    // environment, and factors the `AlbumExpansionContent` no-op-callback
    // boilerplate shared by its previews into one builder.

    extension PreviewData {
        /// A `LibraryStore` seeded with every preview album's summary and its
        /// releases' fat details. `AlbumDetailView.body` finds
        /// `albumSummaries[albumId]` and `releaseDetails[…]` without a live
        /// load, and its `.task` is a no-op because `loadReleaseDetail`
        /// short-circuits when the detail is already present.
        @MainActor
        static func seededLibraryStore() -> LibraryStore {
            let store = LibraryStore()
            for detail in albumDetails.values {
                store.internAlbumDetail(detail)
            }
            return store
        }

        /// The release cursor over an album's releases, preferring one id.
        /// Holds the single empty-`releaseIds` guard for the preview builders.
        @MainActor
        static func releaseCursor(
            releaseIds: [String],
            preferring: String
        ) -> Cursor<ReleaseRef> {
            guard
                let cursor = Cursor(
                    items: releaseIds.map { ReleaseRef(id: $0) },
                    preferring: preferring,
                )
            else {
                fatalError("preview album has empty releaseIds")
            }
            return cursor
        }

        /// Builds an `AlbumExpansionContent` against seeded store data with
        /// no-op callbacks — the shared body of `AlbumExpansionContent`'s
        /// previews. The cursor is a `Binding` so callers can pass a constant
        /// cursor or a live `@State`-backed one (for the release picker to
        /// cycle).
        @MainActor
        static func albumExpansionContent(
            summary: AlbumSummary,
            selectedRelease: ReleaseDetail,
            releaseCursor: Binding<Cursor<ReleaseRef>>,
            currentTrackId: String? = nil,
            loadingTrackId: String? = nil,
            isPlaying: Bool = false,
        ) -> some View {
            AlbumExpansionContent(
                summary: summary,
                selectedRelease: selectedRelease,
                lightboxItems: [],
                releaseCursor: releaseCursor,
                currentTrackId: currentTrackId,
                loadingTrackId: loadingTrackId,
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
                onChangeCover: {},
                onEditMetadata: {},
                onReIdentify: {},
                onManage: {},
                onExportRelease: {},
                onSaveReleaseAs: {},
                onSetPrimaryRelease: {},
                onDeleteRelease: {},
                onExportTrack: { _ in },
            )
        }

        /// The expanded album detail for one seeded fixture album, sized to its
        /// natural width over the dark background — the shared body of
        /// `AlbumExpansionContent`'s previews and the `album-detail` gallery
        /// scene. Seeds a throwaway store from `albumDetails[albumId]`, derives
        /// the summary and primary release, and builds the content over a
        /// constant cursor. Fails loud when the fixture id is unseeded.
        @MainActor
        static func albumExpansionScene(
            albumId: String,
            currentTrackId: String? = nil,
            loadingTrackId: String? = nil,
            isPlaying: Bool = false
        ) -> some View {
            let store = LibraryStore()
            guard let album = albumDetails[albumId] else {
                fatalError("no preview album for id: \(albumId)")
            }
            store.internAlbumDetail(album)
            guard let summary = store.albumSummaries[albumId] else {
                fatalError(
                    "preview seed did not create summary for: \(albumId)"
                )
            }
            guard let primary = store.releaseDetails[summary.primaryReleaseId]
            else {
                fatalError(
                    "no releaseDetail seeded for id: \(summary.primaryReleaseId)"
                )
            }
            let cursor = releaseCursor(
                releaseIds: summary.releaseIds,
                preferring: summary.primaryReleaseId
            )
            return
                albumExpansionContent(
                    summary: summary,
                    selectedRelease: primary,
                    releaseCursor: .constant(cursor),
                    currentTrackId: currentTrackId,
                    loadingTrackId: loadingTrackId,
                    isPlaying: isPlaying
                )
                .padding()
                .frame(width: 1100)
                .background(Theme.background)
                .environment(UiStore())
                .environment(store)
                .environment(MediaPaths.stub)
        }

        /// The primary release's fat detail for a seeded preview album — tracks,
        /// groups, and duration, drawn from the same seed the album-detail tree
        /// reads. Used by the leaf previews that take a `ReleaseDetail` directly
        /// (track list, track row).
        @MainActor
        static func releaseDetail(albumId: String) -> ReleaseDetail {
            let store = seededLibraryStore()
            guard let summary = store.albumSummaries[albumId],
                let detail = store.releaseDetails[summary.primaryReleaseId]
            else {
                fatalError("no preview release detail for album \(albumId)")
            }
            return detail
        }

        /// One `Track` for the `TrackRowView` previews. `displayArtist` non-nil
        /// exercises the compilation row (core sets it only for compilations).
        static func previewTrack(
            id: String = "t-preview",
            title: String = "Track Title",
            position: String = "1",
            durationMs: Int64? = 214_000,
            displayArtist: String? = nil
        ) -> Track {
            Track(
                from: BridgeTrack(
                    id: id,
                    title: title,
                    side: 1,
                    trackNumber: 1,
                    durationMs: durationMs,
                    artistNames: displayArtist ?? "Artist Name",
                    displayArtist: displayArtist,
                    positionText: position
                )
            )
        }

        /// Audio + cover files for the storage sheet's file table.
        static let previewReleaseFiles: [BridgeFile] = [
            BridgeFile(
                id: "f-1",
                originalFilename: "01 - Track Title.flac",
                fileSize: 38_400_000,
                contentType: "Audio",
                isImage: false,
                audioFormat: BridgeAudioFormat(
                    codec: "FLAC",
                    sampleRateHz: 44_100,
                    bitsPerSample: 16,
                    bitrateKbps: nil,
                    channels: 2
                )
            ),
            BridgeFile(
                id: "f-2",
                originalFilename: "02 - Track Title.flac",
                fileSize: 41_100_000,
                contentType: "Audio",
                isImage: false,
                audioFormat: BridgeAudioFormat(
                    codec: "FLAC",
                    sampleRateHz: 44_100,
                    bitsPerSample: 16,
                    bitrateKbps: nil,
                    channels: 2
                )
            ),
            BridgeFile(
                id: "f-cover",
                originalFilename: "cover.jpg",
                fileSize: 2_450_000,
                contentType: "Image",
                isImage: true,
                audioFormat: nil
            ),
        ]

        /// A `ReleaseDetail` in a chosen storage state, for the storage band and
        /// manage-sheet previews: the storage status line, the pin flag, and the
        /// core-computed actions all vary by locality, so each state is its own
        /// fixture rather than a mutation of one.
        @MainActor
        static func storageRelease(
            storageState: BridgeReleaseStorageState,
            pinned: Bool,
            storageActions: [BridgeReleaseStorageAction],
            files: [BridgeFile] = previewReleaseFiles
        ) -> ReleaseDetail {
            let bridge = BridgeRelease(
                id: "rel-storage-preview",
                albumId: "a-storage-preview",
                displayName: "2019 \u{00B7} CD",
                year: 2019,
                format: "CD",
                label: "Some Label",
                catalogNumber: "CAT-0001",
                country: "US",
                storageState: storageState,
                pinned: pinned,
                storageActions: storageActions,
                transferAction: nil,
                tracks: [],
                trackGroups: [],
                files: files,
                imageFiles: [],
                galleryItems: [],
                totalDuration: .minutesOnly(minutes: 39),
                fileCount: Int64(files.count),
                totalSize: files.reduce(Int64(0)) { $0 + $1.fileSize },
                cover: nil
            )
            return ReleaseDetail(
                summary: ReleaseSummary(from: bridge),
                bridge: bridge
            )
        }
    }

    extension View {
        /// Injects every service the `AlbumDetailView` tree reads from the
        /// environment so its real body — and the modal sheets it can present
        /// (cover, edit-metadata, re-identify, storage) — render without a
        /// missing-environment trap. Nine `.stub` domain services plus the
        /// four the modals reach (`Importer`, `ImportStore`, `OutboxStore`,
        /// `ConfigStore`), fresh `PlaybackStore`/`UiStore`, and the passed-in
        /// seeded `LibraryStore`. Nothing about the view is reconstructed.
        @MainActor
        func albumDetailPreviewEnvironment(store: LibraryStore) -> some View {
            self
                .environment(MediaPaths.stub)
                .environment(Playback.stub)
                .environment(Queue.stub)
                .environment(Library.stub)
                .environment(ReleaseEditor.stub)
                .environment(Sync.stub)
                .environment(Downloads.stub)
                .environment(TrackSave.stub)
                .environment(Outputs.stub)
                .environment(Importer.stub)
                .environment(store)
                .environment(PlaybackStore())
                .environment(UiStore())
                .environment(ImportStore())
                .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
                .environment(PreviewData.configStore)
        }
    }
#endif
