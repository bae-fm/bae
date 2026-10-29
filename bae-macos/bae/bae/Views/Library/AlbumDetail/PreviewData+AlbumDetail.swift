#if DEBUG
    import BaeKit
    import SwiftUI

    // Preview support for `AlbumDetailView` and `AlbumExpansionContent`.
    // Answers the detail view's album read from the preview albums, injects
    // the services the view tree pulls from the environment, and factors the
    // `AlbumExpansionContent` no-op-callback boilerplate shared by its
    // previews into one builder.

    extension PreviewData {
        /// A library whose album detail read answers each id with the preview
        /// album of that id, so `AlbumDetailView` runs its real read.
        static func albumDetailLibrary() -> Library {
            Library(
                albumDetail: { .fixed { albumDetails[$0] } },
                releaseFiles: { releaseFilesQuery(previewReleaseFiles) }
            )
        }

        /// A read of a release's files that answers every request with
        /// `files` as they are: a preview has no core to order them.
        static func releaseFilesQuery(_ files: [BridgeFile])
            -> ReleaseFilesQuery
        {
            let requests = PreviewRequests<BridgeReleaseFilesRequest?>()
            return ReleaseFilesQuery(
                setRequest: { requests.send($0) },
                next: {
                    BridgeReleaseFilesSnapshot(
                        request: try await requests.next(),
                        files: files
                    )
                },
                cancel: { requests.finish() }
            )
        }

        /// One preview album as its detail read returns it. Fails loud when
        /// the fixture id is unseeded.
        @MainActor
        static func albumDetail(albumId: String) -> AlbumDetail {
            guard let bridge = albumDetails[albumId] else {
                fatalError("no preview album for id: \(albumId)")
            }
            return AlbumDetail(bridge: bridge)
        }

        /// The release cursor over an album's releases, as the detail view
        /// builds it, preferring one id.
        @MainActor
        static func releaseCursor(
            detail: AlbumDetail,
            preferring: String
        ) -> Cursor<ReleaseRef> {
            guard
                let cursor = Cursor(
                    items: detail.releases.map { ReleaseRef(id: $0.id) },
                    preferring: preferring,
                )
            else {
                fatalError("an album detail holds no release")
            }
            return cursor
        }

        /// Builds an `AlbumExpansionContent` over a preview album with no-op
        /// callbacks — the shared body of `AlbumExpansionContent`'s previews.
        /// The cursor is a `Binding` so callers can pass a constant cursor or a
        /// live `@State`-backed one (for the release picker to cycle).
        @MainActor
        static func albumExpansionContent(
            detail: AlbumDetail,
            selectedRelease: ReleaseDetail,
            releaseCursor: Binding<Cursor<ReleaseRef>>,
            currentTrackId: String? = nil,
            loadingTrackId: String? = nil,
            isPlaying: Bool = false,
        ) -> some View {
            AlbumExpansionContent(
                detail: detail,
                selectedRelease: selectedRelease,
                onBrowseImages: {},
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
                onEdit: {},
                onOpenStorage: {},
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
        /// scene. Builds the content over the album's primary release and a
        /// constant cursor.
        @MainActor
        static func albumExpansionScene(
            albumId: String,
            currentTrackId: String? = nil,
            loadingTrackId: String? = nil,
            isPlaying: Bool = false
        ) -> some View {
            let detail = albumDetail(albumId: albumId)
            let primary = detail.primaryRelease
            let cursor = releaseCursor(detail: detail, preferring: primary.id)
            return
                albumExpansionContent(
                    detail: detail,
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
                .environment(MediaPaths.stub())
                .environment(ImageStore.stub())
        }

        /// The primary release's fat detail for a seeded preview album — tracks,
        /// groups, and duration, drawn from the same seed the album-detail tree
        /// reads. Used by the leaf previews that take a `ReleaseDetail` directly
        /// (track list, track row).
        @MainActor
        static func releaseDetail(albumId: String) -> ReleaseDetail {
            albumDetail(albumId: albumId).primaryRelease
        }

        /// One `Track` for the `TrackRowView` previews. `displayArtist` non-nil
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
                    artistNames: displayArtist ?? "Artist Name",
                    displayArtist: displayArtist,
                    positionText: position
                )
            )
        }

        private static let previewReleaseAudioFormat = BridgeAudioFormat(
            codec: "FLAC",
            sampleRateHz: 44_100,
            bitsPerSample: 16,
            bitrateKbps: nil,
            channels: 2
        )

        private static let previewReleaseCoverFile = BridgeFile(
            id: "f-cover",
            originalFilename: "cover.jpg",
            name: "cover.jpg",
            folder: nil,
            kind: .image(format: "JPEG"),
            fileSize: 2_450_000,
            contentType: "image/jpeg",
            isImage: true,
            audioFormat: nil
        )

        /// Audio + cover files for the storage sheet's file table.
        static let previewReleaseFiles: [BridgeFile] = [
            BridgeFile(
                id: "f-1",
                originalFilename: "01 - Track Title.flac",
                name: "01 - Track Title.flac",
                folder: nil,
                kind: .audio(format: "FLAC"),
                fileSize: 38_400_000,
                contentType: "Audio",
                isImage: false,
                audioFormat: previewReleaseAudioFormat
            ),
            BridgeFile(
                id: "f-2",
                originalFilename: "02 - Track Title.flac",
                name: "02 - Track Title.flac",
                folder: nil,
                kind: .audio(format: "FLAC"),
                fileSize: 41_100_000,
                contentType: "Audio",
                isImage: false,
                audioFormat: previewReleaseAudioFormat
            ),
            previewReleaseCoverFile,
        ]

        /// A `ReleaseDetail` in a chosen storage state, for the storage band and
        /// storage-sheet previews: the storage status line, the pin flag, and the
        /// core-computed actions all vary by locality, so each state is its own
        /// fixture rather than a mutation of one.
        @MainActor
        static func storageRelease(
            storageState: BridgeReleaseStorageState,
            pinned: Bool,
            storageActions: [BridgeReleaseStorageAction]
        ) -> ReleaseDetail {
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
                year: 2019,
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
                pinned: pinned,
                storageActions: storageActions,
                transferAction: nil,
                upload: nil,
                transitioning: false,
                tracks: [],
                trackGroups: [],
                sourceAudio: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .file,
                        format: previewReleaseAudioFormat
                    )
                ),
                imageFiles: [previewReleaseCoverFile],
                coverFiles: [previewReleaseCoverFile],
                galleryItems: [],
                playability: .playable,
                downloadStatus: nil,
                // Every catalog, so the expansion's facts-line popover shows
                // what a release a MusicBrainz pick linked all the way out
                // reads like.
                records: PreviewData.releaseRecordsEveryCatalog,
                totalDuration: .minutesOnly(minutes: 39),
                fileCount: Int64(files.count),
                totalSize: files.reduce(Int64(0)) { $0 + $1.fileSize },
                cover: nil
            )
            return ReleaseDetail(bridge: bridge)
        }
    }

    extension View {
        /// Injects every service the `AlbumDetailView` tree reads from the
        /// environment so its real body — and the modal sheets it can present
        /// (images, the release editor, storage) — render without a
        /// missing-environment trap. Eight `.stub` domain services, a
        /// `Library` whose album read answers from the preview albums, the
        /// four the modals reach (`Importer`, `ImportStore`, `OutboxStore`,
        /// `ConfigStore`), and fresh `PlaybackStore`/`UiStore`. Nothing about
        /// the view is reconstructed.
        @MainActor
        func albumDetailPreviewEnvironment() -> some View {
            self
                .environment(MediaPaths.stub())
                .environment(ImageStore.stub())
                .environment(Playback.stub())
                .environment(Queue.stub())
                .environment(PreviewData.albumDetailLibrary())
                .environment(ReleaseEditor.stub())
                .environment(Sync.stub())
                .environment(Downloads.stub())
                .environment(TrackSave.stub())
                .environment(Outputs.stub())
                .environment(Importer.stub())
                .environment(PlaybackStore())
                .environment(UiStore())
                .environment(ImportStore())
                .environment(OutboxStore(uploadsPaused: false))
                .environment(PreviewData.configStore())
        }
    }

    /// The requests a preview's read is set to, handed to its one reader in
    /// order: what the read answers next.
    final class PreviewRequests<Request: Sendable>: @unchecked Sendable {
        private let continuation: AsyncStream<Request>.Continuation
        private var iterator: AsyncStream<Request>.AsyncIterator

        init() {
            let (stream, continuation) = AsyncStream<Request>.makeStream()
            self.continuation = continuation
            iterator = stream.makeAsyncIterator()
        }

        func send(_ request: Request) {
            continuation.yield(request)
        }

        func next() async throws -> Request {
            guard let request = await iterator.next() else {
                throw CancellationError()
            }
            return request
        }

        func finish() {
            continuation.finish()
        }
    }
#endif
