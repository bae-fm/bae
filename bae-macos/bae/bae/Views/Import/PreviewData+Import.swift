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

        private static let folderImportQueue: BridgeTriageQueue = {
            let rows = [
                triageRow(
                    for: folderCandidates[0],
                    placement: .ready,
                    // The row states the pressing the pane behind it settled
                    // on, so the two halves cannot read differently.
                    matched: triageMatch(
                        releaseId: releaseDetailBridge.releaseId,
                        title: releaseDetailBridge.title,
                        artist: releaseDetailBridge.artist,
                        year: releaseDetailBridge.year,
                        format: releaseDetailBridge.format,
                        trackCount: releaseDetailBridge.trackCount
                    ),
                    selectable: true,
                    importStatus: nil,
                    // Ready means identification settled on one match, which
                    // is a pick — the record a bulk import commits from.
                    picked: .release(
                        source: releaseDetailBridge.source,
                        releaseId: releaseDetailBridge.releaseId,
                        claim: .exact
                    ),
                    claim: claimBridge.choice
                ),
                triageRow(
                    for: folderCandidates[1],
                    placement: .skipped,
                    matched: nil,
                    selectable: false
                ),
                triageRow(
                    for: folderCandidates[2],
                    placement: .done,
                    matched: triageMatch(
                        releaseId: "rel-preview-three",
                        title: "Compilation Vol. 3",
                        trackCount: 15
                    ),
                    selectable: false,
                    importStatus: importStatuses[folderCandidates[2].key],
                    picked: nil,
                    claim: nil
                ),
                triageRow(
                    for: folderCandidates[3],
                    placement: .done,
                    matched: triageMatch(
                        releaseId: "rel-preview-four",
                        title: "EP Release",
                        trackCount: 5
                    ),
                    selectable: false,
                    importStatus: importStatuses[folderCandidates[3].key],
                    picked: nil,
                    claim: nil
                ),
                triageRow(
                    for: folderCandidates[4],
                    placement: .done,
                    matched: triageMatch(
                        releaseId: "rel-preview-five",
                        title: "Live Recording 2023",
                        trackCount: 18
                    ),
                    selectable: false
                ),
                triageRow(
                    for: folderCandidates[5],
                    placement: .ready,
                    matched: triageMatch(
                        releaseId: "rel-preview-six",
                        title: "Album Title Three",
                        year: 2002,
                        trackCount: 11
                    ),
                    selectable: true
                ),
                triageRow(
                    for: folderCandidates[6],
                    placement: .ready,
                    matched: triageMatch(
                        releaseId: "rel-preview-seven",
                        title: "Single Release",
                        year: 1984,
                        format: "7\u{2033}",
                        trackCount: 2,
                        source: .discogs,
                        signal: .barcode
                    ),
                    selectable: true
                ),
            ]
            return BridgeTriageQueue(
                sections: [
                    // Two of the three Ready rows sit under a folder the scan
                    // read as one release's worth of subfolders; the third is
                    // its own folder and sits at the same leading edge.
                    BridgeTriageSection(
                        tab: .ready,
                        watchedFolderPath: importWatchedFolder.path,
                        group: BridgeTriageGroup(
                            key: folderReleaseBoundaryKey,
                            name: "Collection"
                        ),
                        entries: [rows[0], rows[5]].map(candidateEntry)
                    ),
                    BridgeTriageSection(
                        tab: .ready,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: [candidateEntry(rows[6])]
                    ),
                    BridgeTriageSection(
                        tab: .needsYou,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: [boundaryEntry(folderReleaseBoundary)]
                    ),
                    BridgeTriageSection(
                        tab: .done,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: rows[2...4].map(candidateEntry)
                    ),
                    BridgeTriageSection(
                        tab: .skipped,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: [
                            candidateEntry(rows[1]),
                            invalidEntry(invalidCandidates[0]),
                        ]
                    ),
                ],
                counts: BridgeTriageTabCounts(
                    ready: 3,
                    needsYou: 1,
                    done: 3,
                    skipped: 1 + UInt32(invalidCandidates.count)
                ),
                folderScanStatuses: []
            )
        }()

        /// Seeded ImportStore for the ImportView whole-view preview — the
        /// watched folder, every folder candidate, and a triage queue keyed to
        /// the same five so the sidebar and the detail pane agree. ImportStore
        /// is a non-Sendable `@Observable`, so construction is `@MainActor`.
        @MainActor
        static func folderImportStore() -> ImportStore {
            let s = ImportStore()
            s.watchedFolders = [importWatchedFolder]
            for candidate in folderCandidates {
                s.folderCandidates[candidate.key] = candidate
            }
            // The row the preview selects is the settled one, so the pane
            // behind the sidebar is the pane a picked release draws.
            s.folderCandidates[importTabCandidate.key] = importTabCandidate
            s.triageQueue = folderImportQueue
            // Mid-sweep, so the header's progress line is drawn rather than
            // being the nothing-left-to-say state that hides it.
            s.queueIdentifyProgress = (identified: 112, total: 130)
            return s
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
            // Two more Ready folders, so the tab shows a folder group with
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

        static let importStatuses: [String: BridgeCandidateImportStatus] = [
            "/Music/Downloads/Compilation Vol. 3": .importing(
                progressPercent: 45,
                step: .running(phase: .measuringLoudness)
            ),
            // A completed import — tabs under Added via its import status.
            "/Music/Downloads/EP Release": .complete(
                releaseId: "preview-release",
                albumId: "preview-album"
            ),
        ]

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
            )
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

        private static let releaseQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .ready,
                    watchedFolderPath: releaseQueueRoot,
                    group: BridgeTriageGroup(
                        key: releaseQueueGroupKey,
                        name: "Collection"
                    ),
                    entries: releaseQueueRows[0...1].map(candidateEntry)
                ),
                BridgeTriageSection(
                    tab: .ready,
                    watchedFolderPath: releaseQueueRoot,
                    group: nil,
                    entries: [candidateEntry(releaseQueueRows[2])]
                ),
                BridgeTriageSection(
                    tab: .needsYou,
                    watchedFolderPath: releaseQueueRoot,
                    group: nil,
                    entries: [boundaryEntry(releaseQueueBoundary)]
                ),
            ],
            counts: BridgeTriageTabCounts(
                ready: 3,
                needsYou: 1,
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
                    tab: .ready,
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
                ready: 1,
                needsYou: 0,
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
