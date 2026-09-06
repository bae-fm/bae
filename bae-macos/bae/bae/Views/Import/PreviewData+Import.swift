#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the Import flow: watched folders and folder
    /// candidates, the seeded import store, candidate file listings (CUE+FLAC
    /// and per-track), the picked-release detail/seed and its confirm edit, and
    /// the identify/search states (exact, manual, disagreement, triangulating,
    /// not-found) with their signal toolbars. Generic placeholder names
    /// throughout.
    extension PreviewData {
        static let importWatchedFolder = BridgeWatchedFolder(
            path: "/Music/Downloads",
            name: "Downloads"
        )

        // MARK: - Generated placeholder art

        /// Placeholder art for a fixture image: a flat color derived from the
        /// name with the name drawn across it, written as a PNG under the
        /// temporary directory on first use. Fixture paths point here so image
        /// slots decode real bytes instead of settling on the failure
        /// placeholder.
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
                // A generator failure means every slot draws the failure
                // placeholder anyway — crash the preview with the reason
                // instead of rendering a wall of warning triangles.
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

        /// PNG bytes for one placeholder: the name decides the hue, so the
        /// same fixture always renders the same tile and different fixtures
        /// are telling them apart at a glance.
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

        /// ImageStore for previews whose fixtures carry image addresses: a
        /// remote "URL" in fixture data is a path to generated placeholder
        /// art, served straight from disk. The library and release reads stay
        /// unwired exactly like `ImageStore.stub()` — previews have no live
        /// library to read from.
        static func artImageStore() -> ImageStore {
            ImageStore(
                fetchRemoteImage: { url in
                    try Data(contentsOf: URL(fileURLWithPath: url))
                }
            )
        }

        static let folderCandidates: [Candidate] = [
            BridgeFolderCandidate(
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
                folderPath: "/Music/Downloads/Album Title One",
                sourceFolderName: "Album Title One",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 9,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
                folderPath: "/Music/Downloads/Album Title Two [Label CAT-002]",
                sourceFolderName: "Album Title Two",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 12,
                // Skipped example — renders under the Skipped tab.
                skipped: true,
                isAdded: false
            ),
            BridgeFolderCandidate(
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
                folderPath: "/Music/Downloads/Compilation Vol. 3",
                sourceFolderName: "Compilation Vol. 3",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 15,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
                folderPath: "/Music/Downloads/EP Release",
                sourceFolderName: "EP Release",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 5,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                compositionAction: nil,
                combination: nil,
                sourceFileEditsAllowed: true,
                folderPath: "/Music/Downloads/Live Recording 2023",
                sourceFolderName: "Live Recording 2023",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 18,
                // Added example (content-hash match) — renders under the Added tab.
                skipped: false,
                isAdded: true
            ),
            // Two more importable folders, so Pending shows a folder group with
            // rows in it beside a row that belongs to no group.
            BridgeFolderCandidate(
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
                folderPath: "/Music/Downloads/Album Title Three",
                sourceFolderName: "Album Title Three",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 11,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
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

        /// Folders that look like a release but failed validation — surface under
        /// the Skipped tab with a warning and reason.
        static let invalidCandidates: [BridgeInvalidCandidate] = [
            BridgeInvalidCandidate(
                folderPath: "/Music/Downloads/Broken Rip",
                sourceFolderName: "Broken Rip",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Broken Rip",
                resolvedBoundaries: [],
                reason: .corruptAudioFile(path: "03.flac")
            ),
            BridgeInvalidCandidate(
                folderPath: "/Music/Downloads/Damaged Artwork",
                sourceFolderName: "Damaged Artwork",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Damaged Artwork",
                resolvedBoundaries: [],
                reason: .corruptImage(path: "Back.png")
            ),
            BridgeInvalidCandidate(
                folderPath: "/Music/Downloads/Documents Only",
                sourceFolderName: "Documents Only",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Documents Only",
                resolvedBoundaries: [],
                reason: .noValidAudio
            ),
        ]

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
            resolvedBoundaries: [BridgeResolvedFolderReleaseBoundary]
        ) -> BridgeTriageRow {
            BridgeTriageRow(
                candidateKey: "\(releaseQueueRoot)/\(displayPath)",
                folderName: name,
                watchedFolderPath: releaseQueueRoot,
                displayPath: displayPath,
                resolvedBoundaries: resolvedBoundaries,
                combineAncestorKey: nil,
                actionable: true,
                placement: .ready,
                skipAction: .skip,
                actions: [
                    .importReady, .identify, .useFileMetadata, .clearMetadata,
                    .skip,
                ],
                matched: nil,
                metadataSummary: nil,
                coverThumbnail: nil,
                selectable: true,
                importStatus: nil,
                metadataProvenance: nil
            )
        }

        private static let releaseQueueRows = [
            releaseQueueRow(
                name: "Release 01",
                displayPath: "Collection/Release 01",
                resolvedBoundaries: []
            ),
            releaseQueueRow(
                name: "Release 02",
                displayPath: "Collection/Release 02",
                resolvedBoundaries: []
            ),
            releaseQueueRow(
                name: "Release 03",
                displayPath: "Release 03",
                resolvedBoundaries: []
            ),
        ]

        private static let releaseQueueCandidates = releaseQueueRows.map {
            row -> Candidate in
            var candidate = Candidate(
                bridge: BridgeFolderCandidate(
                    compositionAction: .combine,
                    combination: nil,
                    sourceFileEditsAllowed: true,
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
            groupKeys: [releaseQueueGroupKey],
            ready: readyRows(releaseQueueRows)
        )

        private static let releaseQueueResolvedRow = releaseQueueRow(
            name: "Release 01",
            displayPath: "Collection/Release 01",
            resolvedBoundaries: [
                BridgeResolvedFolderReleaseBoundary(
                    key: releaseQueueGroupKey,
                    decision: .keepAsSeparateReleases,
                    name: "Collection",
                    displayPath: "Collection"
                )
            ]
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
                    folderScanStatuses: [
                        BridgeWatchedFolderScanStatus(
                            watchedFolderPath: releaseQueueRoot,
                            watchedFolderName: releaseQueueWatchedFolder.name,
                            status: .scanning(foundCount: 27),
                            onNetworkVolume: false
                        )
                    ],
                    folderScanActivity: BridgeFolderScanActivity(
                        foundCount: 27,
                        folders: [
                            BridgeActiveFolderScan(
                                watchedFolderPath: releaseQueueRoot,
                                watchedFolderName: releaseQueueWatchedFolder
                                    .name,
                                foundCount: 27
                            )
                        ]
                    ),
                    groupKeys: [releaseQueueGroupKey],
                    ready: readyRows(releaseQueueRows)
                )
            )
            scene.store.queueIdentifyProgress = (identified: 27, total: 40)
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
                    watchedFolders: [releaseQueueWatchedFolder],
                    ready: readyRows([releaseQueueResolvedRow])
                )
            )
        }

        /// Every Import-tab state in one production-backed fixture: the
        /// candidate questions and the terminal tabs, under a second watched
        /// root so the folder menu has more than one entry to draw.
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
                    folderScanStatuses: base.folderScanStatuses,
                    groupKeys: base.groupKeys,
                    ready: base.ready,
                    firstUnidentified: base.firstUnidentified
                )
            )
            // The identify count is over the whole queue, with the one row
            // that is still identifying yet to land. Counted off the rows the
            // scene actually holds rather than written down beside them, so
            // adding a row to the fixture cannot leave the header claiming a
            // queue that is not there.
            let queue =
                base.counts.pending + base.counts.done + base.counts.skipped
            scene.store.queueIdentifyProgress = (
                identified: queue - 1, total: queue
            )
            return ImportPreviewFixture(
                store: scene.store,
                itemsByTab: scene.itemsByTab
            )
        }

    }
#endif
