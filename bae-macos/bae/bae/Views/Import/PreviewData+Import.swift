#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the Import flow: watched folders and folder
    /// candidates, the seeded import store, candidate file listings (CUE+FLAC
    /// and per-track), the picked-release detail/seed and its confirm edit, and
    /// the identify/search states (exact, manual, conflict, triangulating,
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
                    RemoteImageBytes(
                        bytes: try Data(contentsOf: URL(fileURLWithPath: url)),
                        validator: url
                    )
                }
            )
        }

        static func candidateEntry(
            _ row: BridgeTriageRow
        ) -> BridgeTriageEntry {
            .candidate(
                stableKey: "candidate:\(row.candidateKey)",
                row: row
            )
        }

        static func boundaryEntry(
            _ boundary: BridgeFolderReleaseBoundary
        ) -> BridgeTriageEntry {
            .boundary(
                stableKey:
                    "boundary:\(boundary.key.watchedFolderPath):"
                    + boundary.key.relativeFolderPath,
                boundary: boundary
            )
        }

        static func invalidEntry(
            _ candidate: BridgeInvalidCandidate
        ) -> BridgeTriageEntry {
            .invalid(
                stableKey: "invalid:\(candidate.folderPath)",
                invalidCandidate: candidate
            )
        }

        static let folderCandidates: [Candidate] = [
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/Album Title One",
                sourceFolderName: "Album Title One",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 9,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
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
                folderPath: "/Music/Downloads/Compilation Vol. 3",
                sourceFolderName: "Compilation Vol. 3",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 15,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/EP Release",
                sourceFolderName: "EP Release",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 5,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
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
                folderPath: "/Music/Downloads/Album Title Three",
                sourceFolderName: "Album Title Three",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 11,
                skipped: false,
                isAdded: false
            ),
            BridgeFolderCandidate(
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

        static let folderReleaseBoundaryKey = BridgeFolderReleaseDecisionKey(
            watchedFolderPath: "/Music/Downloads",
            relativeFolderPath: "Collection"
        )

        static let folderReleaseBoundary = BridgeFolderReleaseBoundary(
            key: folderReleaseBoundaryKey,
            name: "Collection",
            displayPath: "Collection",
            sharedFileCount: 2,
            treeRows: [
                BridgeFolderReleaseTreeRow(
                    name: "Release One",
                    displayPath: "Release One",
                    depth: 0,
                    kind: .candidate(
                        trackCount: 9,
                        formatLabel: "FLAC"
                    ),
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: "/Music/Downloads",
                        relativeFolderPath: "Collection/Release One"
                    ),
                    ancestorDecisionKeys: [folderReleaseBoundaryKey]
                ),
                BridgeFolderReleaseTreeRow(
                    name: "Wrapper",
                    displayPath: "Wrapper",
                    depth: 0,
                    kind: .folder,
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: "/Music/Downloads",
                        relativeFolderPath: "Collection/Wrapper"
                    ),
                    ancestorDecisionKeys: [folderReleaseBoundaryKey]
                ),
                BridgeFolderReleaseTreeRow(
                    name: "Release Two",
                    displayPath: "Wrapper/Release Two",
                    depth: 1,
                    kind: .candidate(
                        trackCount: 12,
                        formatLabel: "FLAC"
                    ),
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: "/Music/Downloads",
                        relativeFolderPath: "Collection/Wrapper/Release Two"
                    ),
                    ancestorDecisionKeys: [
                        folderReleaseBoundaryKey,
                        BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: "/Music/Downloads",
                            relativeFolderPath: "Collection/Wrapper"
                        ),
                    ]
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

        private static let releaseQueueBoundary = BridgeFolderReleaseBoundary(
            key: BridgeFolderReleaseDecisionKey(
                watchedFolderPath: releaseQueueRoot,
                relativeFolderPath: "Archive/Box"
            ),
            name: "Box",
            displayPath: "Archive/Box",
            sharedFileCount: 2,
            treeRows: [
                BridgeFolderReleaseTreeRow(
                    name: "Part 01",
                    displayPath: "Part 01",
                    depth: 0,
                    kind: .candidate(trackCount: 9, formatLabel: "FLAC"),
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: releaseQueueRoot,
                        relativeFolderPath: "Archive/Box/Part 01"
                    ),
                    ancestorDecisionKeys: [
                        BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: releaseQueueRoot,
                            relativeFolderPath: "Archive/Box"
                        )
                    ]
                ),
                BridgeFolderReleaseTreeRow(
                    name: "Part 02",
                    displayPath: "Part 02",
                    depth: 0,
                    kind: .candidate(trackCount: 11, formatLabel: "FLAC"),
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: releaseQueueRoot,
                        relativeFolderPath: "Archive/Box/Part 02"
                    ),
                    ancestorDecisionKeys: [
                        BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: releaseQueueRoot,
                            relativeFolderPath: "Archive/Box"
                        )
                    ]
                ),
                BridgeFolderReleaseTreeRow(
                    name: "Scans",
                    displayPath: "Scans",
                    depth: 0,
                    kind: .folder,
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: releaseQueueRoot,
                        relativeFolderPath: "Archive/Box/Scans"
                    ),
                    ancestorDecisionKeys: [
                        BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: releaseQueueRoot,
                            relativeFolderPath: "Archive/Box"
                        )
                    ]
                ),
                BridgeFolderReleaseTreeRow(
                    name: "Booklet",
                    displayPath: "Scans/Booklet",
                    depth: 1,
                    kind: .folder,
                    decisionKey: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: releaseQueueRoot,
                        relativeFolderPath: "Archive/Box/Scans/Booklet"
                    ),
                    ancestorDecisionKeys: [
                        BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: releaseQueueRoot,
                            relativeFolderPath: "Archive/Box"
                        ),
                        BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: releaseQueueRoot,
                            relativeFolderPath: "Archive/Box/Scans"
                        ),
                    ]
                ),
            ]
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
                matched: nil,
                selectable: true,
                importStatus: nil,
                picked: nil,
                claim: nil
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
                    folderPath: row.candidateKey,
                    sourceFolderName: row.folderName,
                    watchedFolderPath: releaseQueueRoot,
                    files: candidateFilesTracks,
                    trackCount: 9,
                    skipped: false,
                    isAdded: false
                )
            )
            candidate.mapping = awaitingPickTable
            return candidate
        }

        private static let releaseQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: releaseQueueRoot,
                    group: BridgeTriageGroup(
                        key: releaseQueueGroupKey,
                        name: "Collection"
                    ),
                    entries: releaseQueueRows[0...1].map(candidateEntry)
                ),
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: releaseQueueRoot,
                    group: nil,
                    entries: [
                        candidateEntry(releaseQueueRows[2]),
                        boundaryEntry(releaseQueueBoundary),
                    ]
                ),
            ],
            counts: BridgeTriageTabCounts(
                pending: 4,
                done: 0,
                skipped: 0
            ),
            folderScanStatuses: []
        )

        private static let releaseQueueScanning = BridgeTriageQueue(
            sections: releaseQueue.sections,
            counts: releaseQueue.counts,
            folderScanStatuses: [
                BridgeWatchedFolderScanStatus(
                    watchedFolderPath: releaseQueueRoot,
                    watchedFolderName: releaseQueueWatchedFolder.name,
                    status: .scanning
                )
            ]
        )

        private static let releaseQueueResolved = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: releaseQueueRoot,
                    group: nil,
                    entries: [
                        candidateEntry(
                            releaseQueueRow(
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
                        )
                    ]
                )
            ],
            counts: BridgeTriageTabCounts(
                pending: 1,
                done: 0,
                skipped: 0
            ),
            folderScanStatuses: []
        )

        @MainActor
        private static func releaseQueueStore(
            _ queue: BridgeTriageQueue
        ) -> ImportStore {
            let store = ImportStore()
            store.watchedFolders = [releaseQueueWatchedFolder]
            for candidate in releaseQueueCandidates {
                store.folderCandidates[candidate.key] = candidate
            }
            store.triageQueue = queue
            return store
        }

        @MainActor
        static let releaseQueueImportStore = releaseQueueStore(releaseQueue)

        @MainActor
        static func releaseQueueScanningImportStore() -> ImportStore {
            let store = releaseQueueStore(releaseQueueScanning)
            store.queueIdentifyProgress = (identified: 27, total: 40)
            return store
        }

        @MainActor
        static let releaseQueueResolvedImportStore = releaseQueueStore(
            releaseQueueResolved
        )

    }
#endif
