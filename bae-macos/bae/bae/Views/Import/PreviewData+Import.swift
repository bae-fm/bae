#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the Import flow.
    extension PreviewData {
        static let importWatchedFolder = BridgeWatchedFolder(
            path: "/Music/Downloads",
            name: "Downloads"
        )

        // MARK: - Generated placeholder art

        /// A generated placeholder PNG for a fixture image, written to the
        /// temporary directory on first use.
        static func previewArtPath(_ name: String) -> String {
            let directory = URL(fileURLWithPath: NSTemporaryDirectory())
                .appendingPathComponent("bae-preview-art", isDirectory: true)
            let file =
                directory
                .appendingPathComponent(
                    name.replacingOccurrences(of: "/", with: "-")
                )
                .appendingPathExtension("png")
            if !FileManager.default.fileExists(atPath: file.path) {
                // Crash with the reason rather than draw failure placeholders.
                // swiftlint:disable force_try
                try! FileManager.default.createDirectory(
                    at: directory,
                    withIntermediateDirectories: true
                )
                try! previewArtData(name).write(to: file)
                // swiftlint:enable force_try
            }
            return file.path
        }

        /// `previewArtPath`'s file at the version on disk, as core's scan
        /// reports a folder's image.
        static func previewArtFile(_ name: String) -> BridgeFileVersion {
            let path = previewArtPath(name)
            // Crash with the reason rather than draw failure placeholders.
            // swiftlint:disable:next force_try
            let attributes = try! FileManager.default.attributesOfItem(
                atPath: path
            )
            guard let size = attributes[.size] as? UInt64,
                let modified = attributes[.modificationDate] as? Date
            else {
                preconditionFailure("placeholder art has a size and a date")
            }
            return BridgeFileVersion(
                path: path,
                size: size,
                modifiedAtNs: Int64(
                    (modified.timeIntervalSince1970 * 1_000_000_000).rounded()
                )
            )
        }

        /// PNG bytes for one placeholder, its hue decided by the name.
        private static func previewArtData(_ name: String) -> Data {
            let side: CGFloat = 600
            let hash = name.unicodeScalars.reduce(into: UInt32(5381)) {
                $0 = $0 &* 33 &+ $1.value
            }
            let fill = NSColor(
                hue: CGFloat(hash % 360) / 360,
                saturation: 0.35,
                brightness: 0.5,
                alpha: 1
            )
            let image = NSImage(
                size: NSSize(width: side, height: side),
                flipped: false
            ) { rect in
                fill.setFill()
                rect.fill()
                let text = NSAttributedString(
                    string: name,
                    attributes: [
                        .font: NSFont.systemFont(ofSize: 56, weight: .semibold),
                        .foregroundColor: NSColor.white.withAlphaComponent(
                            0.85
                        ),
                    ]
                )
                let textSize = text.size()
                text.draw(
                    at: NSPoint(
                        x: (rect.width - textSize.width) / 2,
                        y: (rect.height - textSize.height) / 2
                    )
                )
                return true
            }
            guard
                let tiff = image.tiffRepresentation,
                let bitmap = NSBitmapImageRep(data: tiff),
                let png = bitmap.representation(using: .png, properties: [:])
            else {
                preconditionFailure("placeholder art must encode as PNG")
            }
            return png
        }

        /// An image store that serves fixture "URLs", which are paths to
        /// generated placeholder art, from disk.
        static func artImageStore() -> ImageStore {
            ImageStore(
                fetchRemoteImage: { image, _ in
                    try Data(contentsOf: URL(fileURLWithPath: image.url))
                }
            )
        }

        static let folderCandidates: [Candidate] = [
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/Album Title One",
                sourceFolderName: "Album Title One",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 9,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/Album Title Two [Label CAT-002]",
                sourceFolderName: "Album Title Two",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 12,
                // Skipped.
                skipped: true,
                isAdded: false
            ),
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/Compilation Vol. 3",
                sourceFolderName: "Compilation Vol. 3",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 15,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/EP Release",
                sourceFolderName: "EP Release",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 5,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/Live Recording 2023",
                sourceFolderName: "Live Recording 2023",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 18,
                // Already imported (content-hash match).
                skipped: false,
                isAdded: true
            ),
            // Two more, so Pending shows a group beside an ungrouped row.
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/Album Title Three",
                sourceFolderName: "Album Title Three",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 11,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                parts: [],
                folderPath: "/Music/Downloads/Single Release",
                sourceFolderName: "Single Release",
                watchedFolderPath: "/Music/Downloads",
                files: candidateFilesTracks,
                trackCount: 2,
                skipped: false,
                isAdded: false
            ),
        ]
        .map(Candidate.init(bridge:))

        /// Folders that failed validation, listed under Skipped: one whose
        /// tags name the release and whose own cover decodes, one with only
        /// its folder's name to go by, and one with no audio at all.
        static let invalidCandidates: [BridgeInvalidCandidate] = [
            BridgeInvalidCandidate(
                candidateKey: "/Music/Downloads/Half Copied",
                folderPath: "/Music/Downloads/Half Copied",
                sourceFolderName: "Half Copied",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Half Copied",
                separable: false,
                reason: .damagedAudio(count: 2),
                tags: BridgeSkippedTags(
                    albumTitle: "Album Title",
                    artist: "Artist Name"
                ),
                cover: .local(file: previewArtFile("Front.png"))
            ),
            BridgeInvalidCandidate(
                candidateKey: "/Music/Downloads/Broken Rip",
                folderPath: "/Music/Downloads/Broken Rip",
                sourceFolderName: "Broken Rip",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Broken Rip",
                separable: false,
                reason: .damagedAudio(count: 1),
                tags: nil,
                cover: nil
            ),
            BridgeInvalidCandidate(
                candidateKey: "/Music/Downloads/Documents Only",
                folderPath: "/Music/Downloads/Documents Only",
                sourceFolderName: "Documents Only",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Documents Only",
                separable: false,
                reason: .noValidAudio,
                tags: nil,
                cover: nil
            ),
        ]

        /// One of a skipped folder's files, as its pane lists it.
        private static func skippedFile(
            folder: String,
            name: String,
            size: UInt64,
            role: BridgeMappingRole
        ) -> BridgeMappingFile {
            let path = "\(folder)/\(name)"
            return BridgeMappingFile(
                fileId: name,
                name: name,
                size: size,
                localPath: path,
                previewTarget: role == .audio
                    ? BridgePreviewTarget(
                        path: path,
                        startSample: 0,
                        endSample: nil
                    ) : nil,
                durationMs: nil,
                audioFormat: nil,
                role: role
            )
        }

        /// The tagged skipped folder looked into: its cover among its images,
        /// two playable tracks, a zero-filled one and an empty one, its rip
        /// log, and a damaged back image.
        static let invalidCandidateDetail: BridgeInvalidCandidateDetail = {
            let folder = invalidCandidates[0].folderPath
            return BridgeInvalidCandidateDetail(
                candidate: invalidCandidates[0],
                images: [
                    BridgeMappingImage(
                        fileId: "Front.png",
                        name: "Front.png",
                        file: previewArtFile("Front.png")
                    )
                ],
                files: [
                    skippedFile(
                        folder: folder,
                        name: "01 Track Title.flac",
                        size: 31_400_000,
                        role: .audio
                    ),
                    skippedFile(
                        folder: folder,
                        name: "02 Track Title.flac",
                        size: 28_900_000,
                        role: .audio
                    ),
                    skippedFile(
                        folder: folder,
                        name: "03 Track Title.flac",
                        size: 33_100_000,
                        role: .damagedAudio(damage: .unreadable)
                    ),
                    skippedFile(
                        folder: folder,
                        name: "04 Track Title.flac",
                        size: 0,
                        role: .damagedAudio(damage: .empty)
                    ),
                    skippedFile(
                        folder: folder,
                        name: "Back.png",
                        size: 512,
                        role: .damagedImage
                    ),
                    skippedFile(
                        folder: folder,
                        name: "rip.log",
                        size: 6_000,
                        role: .document
                    ),
                ]
            )
        }()

        /// The skipped folder with only its name to go by: no tags, no image
        /// that decodes, one empty track.
        static let untaggedInvalidCandidateDetail =
            BridgeInvalidCandidateDetail(
                candidate: invalidCandidates[1],
                images: [],
                files: [
                    skippedFile(
                        folder: invalidCandidates[1].folderPath,
                        name: "01.flac",
                        size: 0,
                        role: .damagedAudio(damage: .empty)
                    ),
                    skippedFile(
                        folder: invalidCandidates[1].folderPath,
                        name: "02.flac",
                        size: 30_200_000,
                        role: .audio
                    ),
                ]
            )

        private static let releaseQueueRoot = "/Music/Incoming"

        static let releaseQueueWatchedFolder = BridgeWatchedFolder(
            path: releaseQueueRoot,
            name: "Incoming"
        )

        static let releaseQueueGroupKey = BridgeFolderReleaseDecisionKey(
            watchedFolderPath: releaseQueueRoot,
            relativeFolderPath: "Collection"
        )

        private static func releaseQueueRow(
            name: String,
            displayPath: String,
            separable: Bool
        ) -> BridgeTriageRow {
            BridgeTriageRow(
                candidateKey: "\(releaseQueueRoot)/\(displayPath)",
                folderName: name,
                watchedFolderPath: releaseQueueRoot,
                displayPath: displayPath,
                actionable: true,
                placement: .pending,
                live: BridgeCandidateLiveState(
                    identifying: nil,
                    readingPick: false,
                    import: nil,
                    actions: [
                        .import, .identify, .resetToFileMetadata,
                        .clearMetadata, .skip,
                        separable ? .separate : .combine, .revealFolder,
                    ],
                    standing: .notLookedUp,
                    badge: nil
                ),
                metadataSummary: nil,
                cover: nil,
                importStatus: nil,
                upload: nil,
                metadataProvenance: nil,
                reading: .unidentified,
                selected: false
            )
        }

        private static let releaseQueueRows = [
            releaseQueueRow(
                name: "Release 01",
                displayPath: "Collection/Release 01",
                separable: false
            ),
            releaseQueueRow(
                name: "Release 02",
                displayPath: "Collection/Release 02",
                separable: false
            ),
            releaseQueueRow(
                name: "Release 03",
                displayPath: "Release 03",
                separable: false
            ),
        ]

        private static let releaseQueueCandidates = releaseQueueRows.map {
            row -> Candidate in
            var candidate = Candidate(
                bridge: BridgeFolderCandidate(
                    parts: [],
                    folderPath: row.candidateKey,
                    sourceFolderName: row.folderName,
                    watchedFolderPath: releaseQueueRoot,
                    files: candidateFilesTracks,
                    trackCount: 9,
                    skipped: false,
                    isAdded: false
                )
            )
            return candidate
        }

        private static let releaseQueueGroupHeader = groupHeaderItem(
            key: releaseQueueGroupKey,
            name: "Collection",
            entryCount: 2
        )

        private static let releaseQueueItems =
            [releaseQueueGroupHeader]
            + releaseQueueRows[0...1].map(candidateItem)
            + [candidateItem(releaseQueueRows[2])]

        private static let releaseQueueSummary = importQueueSummary(
            pending: 3,
            done: 0,
            skipped: 0,
            watchedFolders: [releaseQueueWatchedFolder],
            hasGroups: true
        )

        private static let releaseQueueResolvedRow = releaseQueueRow(
            name: "Release 01",
            displayPath: "Collection/Release 01",
            separable: true
        )

        @MainActor
        private static func releaseQueueScene(
            items: [BridgeImportListItem],
            summary: BridgeImportQueueSummary
        ) -> ImportPreviewFixture {
            let store = ImportStore()
            store.applySummary(summary)
            for candidate in releaseQueueCandidates {
                store.selectedCandidates[candidate.key] = candidate
            }
            return ImportPreviewFixture(
                store: store,
                itemsByTab: [.pending: items, .done: [], .skipped: []]
            )
        }

        @MainActor
        static func releaseQueueScene() -> ImportPreviewFixture {
            releaseQueueScene(
                items: releaseQueueItems,
                summary: releaseQueueSummary
            )
        }

        @MainActor
        static func releaseQueueScanningScene() -> ImportPreviewFixture {
            let scene = releaseQueueScene(
                items: releaseQueueItems,
                summary: importQueueSummary(
                    pending: 3,
                    done: 0,
                    skipped: 0,
                    watchedFolders: [releaseQueueWatchedFolder],
                    hasGroups: true
                )
            )
            scene.store.applyFolderScans(
                BridgeFolderScans(
                    statuses: [
                        BridgeWatchedFolderScanStatus(
                            watchedFolderPath: releaseQueueRoot,
                            watchedFolderName: releaseQueueWatchedFolder.name,
                            status: .scanning(foundCount: 27),
                            onNetworkVolume: false
                        )
                    ],
                    activity: BridgeFolderScanActivity(
                        foundCount: 27,
                        folders: [
                            BridgeActiveFolderScan(
                                watchedFolderPath: releaseQueueRoot,
                                watchedFolderName: releaseQueueWatchedFolder
                                    .name,
                                foundCount: 27
                            )
                        ]
                    )
                )
            )
            scene.store.identificationProgress = (identified: 27, total: 40)
            return scene
        }

        @MainActor
        static func releaseQueueResolvedScene() -> ImportPreviewFixture {
            releaseQueueScene(
                items: [candidateItem(releaseQueueResolvedRow)],
                summary: importQueueSummary(
                    pending: 1,
                    done: 0,
                    skipped: 0,
                    watchedFolders: [releaseQueueWatchedFolder]
                )
            )
        }

        /// Every Import-tab state in one fixture, with a second watched root
        /// so the folder menu has more than one entry.
        @MainActor
        static func importSmokeTestScene() -> ImportPreviewFixture {
            let scene = importTabScene()
            let base = scene.store.summary
            scene.store.applySummary(
                importQueueSummary(
                    pending: base.counts.pending,
                    done: base.counts.done,
                    skipped: base.counts.skipped,
                    watchedFolders: base.watchedFolders
                        + [releaseQueueWatchedFolder],
                    hasGroups: base.hasGroups
                )
            )
            // Counted off the scene's rows, with the one still identifying
            // yet to land.
            let queue =
                base.counts.pending + base.counts.done + base.counts.skipped
            scene.store.identificationProgress = (
                identified: queue - 1, total: queue
            )
            return ImportPreviewFixture(
                store: scene.store,
                itemsByTab: scene.itemsByTab
            )
        }

    }
#endif
