#if DEBUG
    import BaeKit
    import Foundation

    /// A release a preview candidate is picked as: the values the previews
    /// spread over the candidate's link, its row and its cover.
    struct PreviewPickedRelease {
        var releaseId: String
        var source: BridgeCatalog
        var title: String
        var defaultCover: BridgeCoverChoice?
    }

    extension PreviewData {
        private static let previewSourceAudioFormat = BridgeAudioFormat(
            codec: "FLAC",
            sampleRateHz: 44_100,
            bitsPerSample: 16,
            bitrateKbps: nil,
            channels: 2
        )

        private static func previewFile(
            name: String,
            size: UInt64,
            role: BridgeFileRole,
            dirPrefix: String? = nil
        ) -> BridgeCandidateFile {
            BridgeCandidateFile(
                file: BridgeFileInfo(
                    name: dirPrefix.map { $0 + name } ?? name,
                    size: size,
                    dirPrefix: dirPrefix,
                    fileName: name,
                    audioFormat: role.isAudio ? previewSourceAudioFormat : nil
                ),
                role: role
            )
        }

        private static func previewImage(
            name: String,
            size: UInt64,
            dirPrefix: String? = nil
        ) -> BridgeCandidateFile {
            let file = previewArtFile(name)
            let choice = BridgeCoverChoice(
                selection: .releaseImage(fileId: name),
                image: .local(file: file)
            )
            return previewFile(
                name: name,
                size: size,
                role: .artwork(choice: choice),
                dirPrefix: dirPrefix
            )
        }

        /// A sheet bound to `Album Title.flac`, carving the release's nine
        /// tracks out of it.
        static let boundTrackSheet = previewFile(
            name: "Album Title.cue",
            size: 1200,
            role: .trackSheet(trackCount: 9)
        )

        static let mappedAudioContainer = previewFile(
            name: "Album Title.flac",
            size: 340_000_000,
            role: .audio
        )

        static let backImage = previewImage(
            name: "Back.png",
            size: 1_800_000
        )

        static let coverImage = previewImage(
            name: "Front.png",
            size: 2_500_000
        )

        static let scanImage = previewImage(
            name: "scan-1.jpg",
            size: 1_400_000,
            dirPrefix: "scans/"
        )

        /// What core offers a sheet in this folder: the FLAC it can use, and
        /// the MP3 it can't, refused with its codec named.
        static let sheetReferenceOptions = [
            BridgeSheetReferenceOptions(
                fileReference: mappedAudioContainer.file.name,
                fileId: mappedAudioContainer.file.name,
                options: [
                    BridgeSheetBindingOption(
                        fileId: mappedAudioContainer.file.name,
                        offer: .offered
                    ),
                    BridgeSheetBindingOption(
                        fileId: "Album Title.mp3",
                        offer: .refusedCodec(codec: "MP3")
                    ),
                ]
            )
        ]

        static let infoLog = previewFile(
            name: "info.log",
            size: 6000,
            role: .document
        )

        static let notesDocument = previewFile(
            name: "notes.txt",
            size: 1200,
            role: .document
        )

        /// An image whose bytes do not decode: carried, never artwork.
        static let damagedFrontImage = previewFile(
            name: "front.png",
            size: 512,
            role: .damagedImage
        )

        static let supplementalVideo = previewFile(
            name: "video.mkv",
            size: 24_000_000,
            role: .other
        )

        static let previewLogDocuments = [
            "checksum.txt",
            "drive.txt",
            "read.txt",
            "verify.txt",
        ]
        .map {
            previewFile(
                name: $0,
                size: 6000,
                role: .document,
                dirPrefix: "logs/"
            )
        }

        static let bridgeCandidateFiles = BridgeCandidateFiles(
            fileMetadataIdentity: "cue-backed-preview-audio",
            files: [
                backImage,
                boundTrackSheet,
                mappedAudioContainer,
                coverImage,
                scanImage,
                infoLog,
                notesDocument,
                damagedFrontImage,
                supplementalVideo,
            ]
                + previewLogDocuments,
            coverFiles: [backImage, coverImage, scanImage],
            sourceAudio: BridgeCandidateSourceAudio(
                summary: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .cue,
                        format: previewSourceAudioFormat
                    )
                ),
                files: [mappedAudioContainer.file]
            )
        )

        static let releaseDetailBridge = PreviewPickedRelease(
            releaseId: "rel-123",
            source: .musicBrainz,
            title: "Album Title One",
            defaultCover: BridgeCoverChoice(
                selection: .releaseImage(fileId: "Front.png"),
                image: .local(file: previewArtFile("Front.png"))
            )
        )

        /// Editor seed for the confirming previews — the raw release edit the
        /// prefetch's seed projects into.
        static let confirmEditValues = editMetadataDraft(trackCount: 9)

        /// Per-track audio candidate (nine FLAC files) plus one cover image, two
        /// documents, and a sheet describing nothing yet — the file-per-track
        /// counterpart to `bridgeCandidateFiles`.
        private static let trackAudioFiles: [BridgeCandidateFile] = (1...9)
            .map { (i: Int) -> BridgeCandidateFile in
                previewFile(
                    name: "Track \(i).flac",
                    size: UInt64(35_000_000 + i * 2_000_000),
                    role: .audio
                )
            }

        static let candidateFilesTracks = BridgeCandidateFiles(
            fileMetadataIdentity: "file-backed-preview-audio",
            files: trackAudioFiles
                + [
                    coverImage,
                    infoLog,
                    notesDocument,
                    previewFile(
                        name: "Album.cue",
                        size: 1100,
                        role: .trackSheet(trackCount: 9)
                    ),
                ],
            coverFiles: [coverImage],
            sourceAudio: BridgeCandidateSourceAudio(
                summary: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .file,
                        format: previewSourceAudioFormat
                    )
                ),
                files: trackAudioFiles.map(\.file)
            )
        )

    }
#endif
