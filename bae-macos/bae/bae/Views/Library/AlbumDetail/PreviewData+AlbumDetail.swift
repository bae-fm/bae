#if DEBUG
    import BaeKit
    import SwiftUI

    // Fixture support for `AlbumDetailView` and `AlbumExpansionContent`.
    // Answers the detail view's album read from the shot albums, injects
    // the services the view tree pulls from the environment, and factors the
    // `AlbumExpansionContent` no-op-callback boilerplate shared by its
    // shots into one builder.

    extension PreviewData {
        /// A library whose album detail read answers each id with the shot
        /// album of that id, so `AlbumDetailView` runs its real read.
        static func albumDetailLibrary() -> Library {
            Library(
                albumDetail: {
                    .fixed(answers: UnansweredCount()) { albumDetails[$0] }
                },
                releaseDetail: {
                    .fixed(answers: UnansweredCount()) { id in
                        albumDetails.values.lazy.flatMap(\.releases)
                            .first { $0.id == id }
                    }
                },
                releaseFiles: {
                    .fixed(previewReleaseFiles, answers: UnansweredCount())
                }
            )
        }

        static let previewReleaseAudioFormat = BridgeAudioFormat(
            codec: "FLAC",
            sampleRateHz: 44_100,
            bitsPerSample: 16,
            bitrateKbps: nil,
            channels: 2
        )

        static let previewReleaseCoverFile = BridgeFile(
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
    }

    extension View {
        /// Injects every service the `AlbumDetailView` tree reads from the
        /// environment so its real body — and the modal sheets it can present
        /// (images, the release editor, storage) — render without a
        /// missing-environment trap. Eight `.stub` domain services, a
        /// `Library` whose album read answers from the shot albums, the
        /// projection store over it, the four the modals reach (`Importer`,
        /// `ImportStore`, `OutboxStore`, `ConfigStore`), and fresh
        /// `PlaybackStore`/`UiStore`. Nothing about the view is
        /// reconstructed.
        @MainActor
        func albumDetailPreviewEnvironment() -> some View {
            let library = PreviewData.albumDetailLibrary()
            return
                self
                .environment(ImageStore.stub())
                .environment(Playback.stub())
                .environment(Queue.stub())
                .environment(library)
                .environment(LibraryProjectionStore(library: library))
                .environment(ReleaseEditor.stub())
                .environment(Sync.stub())
                .environment(Downloads.stub())
                .environment(TrackSave.stub())
                .environment(Outputs.stub())
                .environment(Importer.stub())
                .environment(PlaybackStore())
                .environment(UiStore())
                .environment(SettingsNavigation())
                .environment(\.openSettingsWindow, OpenSettingsWindowAction {})
                .environment(ImportStore())
                .environment(OutboxStore(uploadsPaused: false))
                .environment(PreviewData.configStore())
        }
    }
#endif
