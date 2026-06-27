#if DEBUG
    import SwiftUI

    // Preview support for `AlbumDetailView` and `AlbumExpansionContent`.
    // Seeds a `LibraryStore` with the album summaries and release details the
    // detail view reads, injects the services the view tree pulls from the
    // environment, and factors the `AlbumExpansionContent` no-op-callback
    // boilerplate shared by its previews into one builder.

    extension PreviewData {
        /// A `LibraryStore` seeded with every preview album's summary and its
        /// releases' fat details, via the same `handleAlbumAdded` reducer the
        /// app runs on an `AlbumAdded` event. `AlbumDetailView.body` finds
        /// `albumSummaries[albumId]` and `releaseDetails[…]` without a live
        /// load, and its `.task` is a no-op because `loadReleaseDetail`
        /// short-circuits when the detail is already present.
        @MainActor
        static func seededLibraryStore() -> LibraryStore {
            let store = LibraryStore()
            for detail in albumDetails.values {
                store.handleAlbumAdded(album: detail)
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
                onSetPrimaryRelease: {},
                onDeleteRelease: {},
                onExportTrack: { _ in },
            )
        }
    }

    extension View {
        /// Injects every service the `AlbumDetailView` tree reads from the
        /// environment so its real body — and the modal sheets it can present
        /// (cover, edit-metadata, re-identify, storage) — render without a
        /// missing-environment trap. Eight `.stub` domain services plus the
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
                .environment(Export.stub)
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
