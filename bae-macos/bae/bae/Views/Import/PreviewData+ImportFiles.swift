#if DEBUG
    import BaeKit
    import Foundation

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
            becomes: BridgeFileBecomes = .noSlots,
            dirPrefix: String? = nil,
            localPath: String? = nil
        ) -> BridgeCandidateFile {
            // Only audio is a decision, so only audio carries alternatives —
            // the same rule core applies.
            BridgeCandidateFile(
                file: BridgeFileInfo(
                    name: dirPrefix.map { $0 + name } ?? name,
                    size: size,
                    dirPrefix: dirPrefix,
                    fileName: name,
                    localPath: localPath ?? "/tmp/fake/\(name)",
                    audioFormat: role.isAudio ? previewSourceAudioFormat : nil
                ),
                role: role,
                becomes: becomes,
                alternatives: role.isAudio ? [.audio, .notATrack] : [],
                roleChoice: role.isAudio ? .audio : nil
            )
        }

        private static func previewImage(
            name: String,
            size: UInt64,
            dirPrefix: String? = nil
        ) -> BridgeCandidateFile {
            let path = previewArtPath(name)
            let choice = BridgeCoverChoice(
                selection: .releaseImage(fileId: name),
                previewSource: .local(path: path),
                thumbnailSource: .local(path: path)
            )
            return previewFile(
                name: name,
                size: size,
                role: .artwork(choice: choice),
                dirPrefix: dirPrefix,
                localPath: path
            )
        }

        /// A sheet bound to `Album Title.flac`, carving the release's nine
        /// slots out of it.
        static let boundTrackSheet = previewFile(
            name: "Album Title.cue",
            size: 1200,
            role: .trackSheet(trackCount: 9),
            becomes: .slots(first: 1, last: 9)
        )

        static let mappedAudioContainer = previewFile(
            name: "Album Title.flac",
            size: 340_000_000,
            role: .audio,
            becomes: .slots(first: 1, last: 9)
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
        static let sheetBindingOptions:
            [String: [BridgeSheetReferenceOptions]] = [
                boundTrackSheet.file.name: [
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
            fileTagsIdentity: "cue-backed-preview-audio",
            files: [
                backImage,
                boundTrackSheet,
                mappedAudioContainer,
                coverImage,
                scanImage,
                infoLog,
                notesDocument,
                supplementalVideo,
            ]
                + previewLogDocuments,
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

        static let releaseDetailBridge: BridgeReleaseDetail = {
            let tracks: [BridgeReleaseTrack] = (1...9)
                .map { i in
                    let ms = UInt64(180_000 + i * 15000)
                    return BridgeReleaseTrack(
                        title: "Track Title \(i)",
                        artist: i == 5 ? "Featured Artist" : nil,
                        durationMs: ms,
                        position: "\(i)",
                        side: 1
                    )
                }
            return BridgeReleaseDetail(
                releaseId: "rel-123",
                source: .musicBrainz,
                sourceGroupId: "rg-123",
                title: "Album Title One",
                artist: "Artist Name",
                year: 1996,
                format: "CD",
                label: "Label Name",
                catalogNumber: "6006-2",
                country: "US",
                barcode: nil,
                trackCount: 9,
                tracks: tracks,
                coverArt: [],
                defaultCover: BridgeCoverChoice(
                    selection: .releaseImage(fileId: "Front.png"),
                    previewSource: .local(path: previewArtPath("Front.png")),
                    thumbnailSource: .local(path: previewArtPath("Front.png"))
                )
            )
        }()

        /// Editor seed for the confirming previews — the raw release edit the
        /// prefetch's seed projects into.
        static let confirmEditValues = editMetadataDraft(trackCount: 9)

        /// Per-track audio candidate (nine FLAC files) plus one cover image, two
        /// documents, and a sheet describing nothing yet — the file-per-track
        /// counterpart to `bridgeCandidateFiles`.
        private static let trackAudioFiles: [BridgeCandidateFile] = (1...9)
            .map { (i: Int) -> BridgeCandidateFile in
                let slot = UInt32(i)
                return previewFile(
                    name: "Track \(i).flac",
                    size: UInt64(35_000_000 + i * 2_000_000),
                    role: .audio,
                    becomes: .slots(first: slot, last: slot)
                )
            }

        static let candidateFilesTracks = BridgeCandidateFiles(
            fileTagsIdentity: "file-backed-preview-audio",
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
