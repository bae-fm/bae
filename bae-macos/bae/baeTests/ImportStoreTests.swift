import BaeKit
import Testing

@testable import bae

private func makeCandidate(_ key: String) -> Candidate {
    Candidate(
        reIdentifyKey: key,
        releaseId: "existing-release",
        displayName: "Candidate \(key)"
    )
}

private func makeStatus(albumId: String) -> BridgeLibraryStatus {
    BridgeLibraryStatus(
        releaseId: "unused",
        releaseInLibrary: true,
        albumInLibrary: true,
        albumTitle: "Album Title",
        albumId: albumId
    )
}

// MARK: - Bridge snapshot builders

/// The Bridge* records are generated at build from `bae-bridge/src/types.rs`;
/// these hand-build the minimal shapes the snapshot reducers consume.

private func emptyBridgeFiles() -> BridgeCandidateFiles {
    BridgeCandidateFiles(files: [], formatLabel: "", collapsedDirectories: [])
}

private func idleRuntime(
    importStatus: BridgeCandidateImportStatus? = nil
) -> BridgeCandidateRuntimeSnapshot {
    BridgeCandidateRuntimeSnapshot(
        identifyState: .idle,
        signalsToolbar: BridgeSignalsToolbar(signals: []),
        signals: nil,
        importStatus: importStatus
    )
}

private func bridgeFolder(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    trackCount: UInt32 = 10,
    skipped: Bool = false,
    isAdded: Bool = false
) -> BridgeFolderCandidate {
    BridgeFolderCandidate(
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        files: emptyBridgeFiles(),
        trackCount: trackCount,
        skipped: skipped,
        isAdded: isAdded
    )
}

/// A folder-source `Candidate` with the given scan flags and (optional) session
/// import status, for the batch/single-snapshot suites.
private func folderCandidate(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    isAdded: Bool = false,
    importStatus: BridgeCandidateImportStatus? = nil
) -> Candidate {
    var candidate = Candidate(
        bridge: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped,
            isAdded: isAdded
        )
    )
    candidate.importStatus = importStatus
    return candidate
}

private func bridgeInvalid(
    folderPath: String,
    watchedFolderPath: String,
    name: String
) -> BridgeInvalidCandidate {
    BridgeInvalidCandidate(
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        displayPath: name,
        resolvedBoundaries: [],
        reason: .noValidAudio
    )
}

private func candidateEntry(_ row: BridgeTriageRow) -> BridgeTriageEntry {
    .candidate(stableKey: "candidate:\(row.candidateKey)", row: row)
}

private func invalidEntry(
    _ candidate: BridgeInvalidCandidate
) -> BridgeTriageEntry {
    .invalid(
        stableKey: "invalid:\(candidate.folderPath)",
        invalidCandidate: candidate
    )
}

private func boundaryEntry(
    key: String,
    displayPath: String
) -> BridgeTriageEntry {
    .boundary(
        stableKey: "boundary:\(key)",
        boundary: BridgeFolderReleaseBoundary(
            key: BridgeFolderReleaseDecisionKey(
                watchedFolderPath: "/w",
                relativeFolderPath: key
            ),
            name: key,
            displayPath: displayPath,
            sharedFileCount: 0,
            treeRows: []
        )
    )
}

// MARK: - Triage row builders

private func matchedRelease(
    releaseId: String,
    title: String,
    trackCount: UInt32? = 10,
    coverThumbnailUrl: String? = nil
) -> BridgeMatchedRelease {
    BridgeMatchedRelease(
        releaseId: releaseId,
        title: title,
        artist: "Artist",
        pressing: trackCount.map {
            BridgeMatchedPressing(year: 2000, format: "CD", trackCount: $0)
        },
        coverThumbnailUrl: coverThumbnailUrl,
        evidence: BridgeMatchEvidence(source: .musicBrainz, signal: .discId)
    )
}

/// A Ready row: matched, selectable, no import status.
private func readyRow(
    _ key: String,
    title: String,
    coverThumbnailUrl: String? = nil
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .ready,
        skipAction: .skip,
        matched: matchedRelease(
            releaseId: "rel-\(key)",
            title: title,
            coverThumbnailUrl: coverThumbnailUrl
        ),
        selectable: true,
        importStatus: nil,
        picked: nil,
        claim: nil
    )
}

/// A Needs-you row in `group`, never selectable.
private func needsYouRow(
    _ key: String,
    title: String,
    group: BridgeNeedsYouGroup,
    reason: BridgeNeedsYouReason
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .needsYou(group: group, reason: reason),
        skipAction: .skip,
        matched: nil,
        selectable: false,
        importStatus: nil,
        picked: nil,
        claim: nil
    )
}

private func doneRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .done,
        skipAction: nil,
        matched: matchedRelease(releaseId: "rel-\(key)", title: title),
        selectable: false,
        importStatus: .complete(releaseId: "rel-\(key)", albumId: "al-\(key)"),
        picked: nil,
        claim: nil
    )
}

private func skippedRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .skipped,
        skipAction: .unskip,
        matched: nil,
        selectable: false,
        importStatus: nil,
        picked: nil,
        claim: nil
    )
}

private func bridgeRow(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    resumedIdentifyState: BridgeIdentifyState = .idle
) -> BridgeFolderImportCandidateSnapshot {
    BridgeFolderImportCandidateSnapshot(
        candidate: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped
        ),
        actionable: true,
        resumedIdentifyState: resumedIdentifyState
    )
}

private func listSnapshot(
    watchedFolders: [BridgeWatchedFolder] = [
        BridgeWatchedFolder(path: "/w1", name: "w1")
    ],
    folderCandidates: [BridgeFolderImportCandidateSnapshot],
    invalidCandidates: [BridgeInvalidCandidate] = []
) -> BridgeImportCandidatesSnapshot {
    BridgeImportCandidatesSnapshot(
        watchedFolders: watchedFolders,
        folderCandidates: folderCandidates,
        invalidCandidates: invalidCandidates,
        boundaries: [],
        folderScanStatuses: []
    )
}

@Suite("ImportStore.applyImportCandidatesSnapshot")
struct ImportStoreListSnapshotTests {
    @MainActor
    @Test("inserts fresh candidates and watched folders")
    func insertsFresh() throws {
        let store = ImportStore()

        store.applyImportCandidatesSnapshot(
            listSnapshot(
                folderCandidates: [
                    bridgeRow(
                        folderPath: "/w1/a",
                        watchedFolderPath: "/w1",
                        name: "A",
                        resumedIdentifyState: .notFoundAnywhere
                    )
                ],
                invalidCandidates: [
                    bridgeInvalid(
                        folderPath: "/w1/bad",
                        watchedFolderPath: "/w1",
                        name: "Bad"
                    )
                ]
            )
        )

        #expect(store.watchedFolders.map(\.path) == ["/w1"])
        let fresh = try #require(store.folderCandidates["/w1/a"])
        #expect(fresh.displayName == "A")
        // A fresh candidate with no run live shows its resumed state.
        #expect(fresh.identifyState == .notFoundAnywhere)
        #expect(fresh.importStatus == nil)
    }

    @MainActor
    @Test("a re-read list keeps runtime and editor state on a surviving key")
    func keepsRuntimeAndEditorState() throws {
        let store = ImportStore()
        var existing = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        existing.identity = .unknown
        existing.importStatus = .complete(releaseId: "rel-1", albumId: "al-1")
        existing.runtimeIdentifyState = .triangulating(
            discid: .computing,
            barcode: .scanning
        )
        existing.reconcileIdentifyState()
        existing.signals = Signals(
            text: .settled(catalogs: ["CAT-1"], freeText: [])
        )
        existing.signalsToolbar = PreviewData.toolbarBothRunning
        existing.libraryStatuses = ["rel-1": makeStatus(albumId: "al-1")]
        store.folderCandidates["/w1/a"] = existing

        // Same key, renamed + skip flipped.
        store.applyImportCandidatesSnapshot(
            listSnapshot(
                folderCandidates: [
                    bridgeRow(
                        folderPath: "/w1/a",
                        watchedFolderPath: "/w1",
                        name: "A-renamed",
                        skipped: true
                    )
                ]
            )
        )

        let merged = try #require(store.folderCandidates["/w1/a"])
        // Editor/session state and the run in flight survive; the list only
        // re-read the folder.
        #expect(merged.identity == .unknown)
        #expect(
            merged.importStatus
                == .complete(releaseId: "rel-1", albumId: "al-1")
        )
        #expect(
            merged.identifyState
                == .triangulating(discid: .computing, barcode: .scanning)
        )
        #expect(merged.signals != nil)
        #expect(!merged.signalsToolbar.signals.isEmpty)
        #expect(merged.libraryStatuses["rel-1"] != nil)
        // Scan fields come from the incoming list.
        #expect(merged.displayName == "A-renamed")
        #expect(merged.files.files.isEmpty)
    }

    @MainActor
    @Test("drops candidates absent from the new list")
    func dropsAbsent() {
        let store = ImportStore()
        store.folderCandidates["/w1/a"] = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        store.folderCandidates["/w1/b"] = folderCandidate(
            folderPath: "/w1/b",
            watchedFolderPath: "/w1",
            name: "B"
        )

        store.applyImportCandidatesSnapshot(
            listSnapshot(
                folderCandidates: [
                    bridgeRow(
                        folderPath: "/w1/a",
                        watchedFolderPath: "/w1",
                        name: "A"
                    )
                ]
            )
        )

        #expect(store.folderCandidates["/w1/a"] != nil)
        #expect(store.folderCandidates["/w1/b"] == nil)
    }
}

@Suite("ImportStore.applyCandidateRuntimeChange")
struct ImportStoreRuntimeChangeTests {
    @MainActor
    @Test("a runtime change updates one folder candidate")
    func updatesOneCandidate() throws {
        let store = ImportStore()
        store.applyImportCandidatesSnapshot(
            listSnapshot(
                folderCandidates: [
                    bridgeRow(
                        folderPath: "/w1/a",
                        watchedFolderPath: "/w1",
                        name: "A",
                        resumedIdentifyState: .notFoundAnywhere
                    ),
                    bridgeRow(
                        folderPath: "/w1/b",
                        watchedFolderPath: "/w1",
                        name: "B"
                    ),
                ]
            )
        )

        store.applyCandidateRuntimeChange(
            .updated(
                key: "/w1/a",
                runtime: BridgeCandidateRuntimeSnapshot(
                    identifyState: .triangulating(
                        discid: .computing,
                        barcode: .scanning
                    ),
                    signalsToolbar: BridgeSignalsToolbar(signals: []),
                    signals: nil,
                    importStatus: .importing(progressPercent: 40, step: nil)
                )
            )
        )

        let running = try #require(store.folderCandidates["/w1/a"])
        // A live run outranks the resumed state.
        #expect(
            running.identifyState
                == .triangulating(discid: .computing, barcode: .scanning)
        )
        #expect(
            running.importStatus == .importing(progressPercent: 40, step: nil)
        )
        #expect(store.folderCandidates["/w1/b"]?.importStatus == nil)

        store.applyCandidateRuntimeChange(.removed(key: "/w1/a"))
        let cleared = try #require(store.folderCandidates["/w1/a"])
        #expect(cleared.importStatus == nil)
        // With no run live the resumed state shows again.
        #expect(cleared.identifyState == .notFoundAnywhere)
    }

    @MainActor
    @Test("runtime that arrives before its row joins the row when it lands")
    func runtimeAheadOfList() throws {
        let store = ImportStore()
        store.applyCandidateRuntimeChange(
            .updated(
                key: "/w1/a",
                runtime: idleRuntime(
                    importStatus: .importing(progressPercent: 5, step: nil)
                )
            )
        )
        #expect(store.folderCandidates["/w1/a"] == nil)

        store.applyImportCandidatesSnapshot(
            listSnapshot(
                folderCandidates: [
                    bridgeRow(
                        folderPath: "/w1/a",
                        watchedFolderPath: "/w1",
                        name: "A"
                    )
                ]
            )
        )

        let landed = try #require(store.folderCandidates["/w1/a"])
        #expect(
            landed.importStatus == .importing(progressPercent: 5, step: nil)
        )
    }

    @MainActor
    @Test("a runtime change updates a re-identify session")
    func updatesReIdentifySession() {
        let store = ImportStore()
        store.reIdentifyCandidates["reidentify:r1"] =
            makeCandidate("reidentify:r1")

        store.applyCandidateRuntimeChange(
            .updated(
                key: "reidentify:r1",
                runtime: idleRuntime(
                    importStatus: .complete(
                        releaseId: "rel-1",
                        albumId: "al-1"
                    )
                )
            )
        )

        #expect(
            store.reIdentifyCandidates["reidentify:r1"]?.importStatus
                == .complete(releaseId: "rel-1", albumId: "al-1")
        )
    }
}

@Suite("Import loudness progress")
struct ImportLoudnessProgressTests {
    @MainActor
    @Test("preserves unavailable fractions as indeterminate")
    func preservesIndeterminateProgress() throws {
        let store = ImportStore()
        let handler = DesktopEventHandler(
            importStore: store,
            mediaControlService: MediaControlService()
        )

        handler.apply(
            .candidateImportLoudnessProgress(
                key: "candidate",
                tracksDone: 1,
                tracksTotal: 3,
                fraction: nil
            )
        )

        let event = try #require(store.importLoudnessSubject.value)
        #expect(event.key == "candidate")
        #expect(event.tracksDone == 1)
        #expect(event.tracksTotal == 3)
        #expect(event.fraction == nil)
    }
}

// MARK: - Triage rendering

@Suite("ImportStore triage rendering")
struct ImportStoreTriageRenderingTests {
    private func seededStore() -> ImportStore {
        let store = ImportStore()
        store.triageQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: [
                        candidateEntry(readyRow("/w/ready-b", title: "Beta")),
                        candidateEntry(readyRow("/w/ready-a", title: "Alpha")),
                    ]
                ),
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: "/w",
                    group: BridgeTriageGroup(
                        key: BridgeFolderReleaseDecisionKey(
                            watchedFolderPath: "/w",
                            relativeFolderPath: "collection"
                        ),
                        name: "Collection"
                    ),
                    entries: [
                        candidateEntry(
                            needsYouRow(
                                "/w/collection/first",
                                title: "First",
                                group: .pickAPressing,
                                reason: .disagreement(
                                    disagreement: .severalMatches(count: 2)
                                )
                            )
                        ),
                        candidateEntry(
                            needsYouRow(
                                "/w/collection/second",
                                title: "Second",
                                group: .signalsDisagree,
                                reason: .disagreement(
                                    disagreement: .signalsConflict
                                )
                            )
                        ),
                    ]
                ),
                BridgeTriageSection(
                    tab: .done,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: [
                        candidateEntry(doneRow("/w/done", title: "Finished"))
                    ]
                ),
                BridgeTriageSection(
                    tab: .skipped,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: [
                        candidateEntry(
                            skippedRow(
                                "/w/skipped",
                                title: "Set Aside"
                            )
                        ),
                        invalidEntry(
                            bridgeInvalid(
                                folderPath: "/w/bad",
                                watchedFolderPath: "/w",
                                name: "Bad Folder"
                            )
                        ),
                    ]
                ),
            ],
            counts: BridgeTriageTabCounts(
                pending: 4,
                done: 1,
                skipped: 2
            ),
            folderScanStatuses: []
        )
        return store
    }

    @MainActor
    @Test("filtering preserves core order when matched titles change")
    func filteringPreservesCoreOrderAcrossSnapshots() {
        let store = ImportStore()
        let firstEntries: [BridgeTriageEntry] = [
            candidateEntry(readyRow("/w/01", title: "Zulu Common")),
            boundaryEntry(key: "02 Common", displayPath: "02 Common"),
            candidateEntry(readyRow("/w/03", title: "Alpha Common")),
        ]
        store.triageQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: firstEntries
                )
            ],
            counts: BridgeTriageTabCounts(pending: 3, done: 0, skipped: 0),
            folderScanStatuses: []
        )

        #expect(
            store.releaseSections(
                tab: .pending,
                filterText: "common"
            )
            .flatMap(\.entries).map(\.id)
                == firstEntries.map(ReleaseQueueEntry.init).map(\.id)
        )

        let secondEntries: [BridgeTriageEntry] = [
            candidateEntry(readyRow("/w/01", title: "Alpha Common")),
            boundaryEntry(key: "02 Common", displayPath: "02 Common"),
            candidateEntry(readyRow("/w/03", title: "Zulu Common")),
        ]
        store.triageQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: secondEntries
                )
            ],
            counts: BridgeTriageTabCounts(pending: 3, done: 0, skipped: 0),
            folderScanStatuses: []
        )

        #expect(
            store.releaseSections(
                tab: .pending,
                filterText: "common"
            )
            .flatMap(\.entries).map(\.id)
                == secondEntries.map(ReleaseQueueEntry.init).map(\.id)
        )
    }

    @MainActor
    @Test("sections preserve grouping and entry order")
    func sectionsPreserveGrouping() throws {
        let store = seededStore()
        let importable = try #require(
            store.releaseSections(
                tab: .pending,
                filterText: ""
            )
            .first { $0.group == nil }
        )
        #expect(
            importable.entries.compactMap { entry in
                guard
                    case .candidate(_, let row) = entry.bridge
                else { return nil }
                return row.folderName
            }
                == ["Beta", "Alpha"]
        )
        #expect(
            importable.entries.map(\.id)
                == [
                    "candidate:/w/ready-b",
                    "candidate:/w/ready-a",
                ]
        )

        let grouped = try #require(
            store.releaseSections(
                tab: .pending,
                filterText: ""
            )
            .first { $0.group != nil }
        )
        #expect(grouped.group?.name == "Collection")
        #expect(
            grouped.group?.key.relativeFolderPath == "collection"
        )
    }

    @MainActor
    @Test("filter keeps matching entries inside their section")
    func filterKeepsMatchingSectionEntries() throws {
        let section = try #require(
            seededStore()
                .releaseSections(
                    tab: .skipped,
                    filterText: "bad"
                )
                .first
        )
        #expect(section.entries.count == 1)
        guard
            case .invalid(_, let invalid) = section.entries[0].bridge
        else {
            Issue.record("expected invalid entry")
            return
        }
        #expect(invalid.folderPath == "/w/bad")
    }

    @MainActor
    @Test("an answered row stays in Pending and becomes selectable")
    func answeringMakesThePendingRowSelectable() {
        let store = ImportStore()
        store.triageQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: [
                        candidateEntry(
                            needsYouRow(
                                "/w/subject",
                                title: "Subject",
                                group: .pickAPressing,
                                reason: .disagreement(
                                    disagreement: .severalMatches(count: 2)
                                )
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

        #expect(
            store.releaseSections(
                tab: .pending,
                filterText: ""
            )
            .count == 1
        )

        store.triageQueue = BridgeTriageQueue(
            sections: [
                BridgeTriageSection(
                    tab: .pending,
                    watchedFolderPath: "/w",
                    group: nil,
                    entries: [
                        candidateEntry(
                            readyRow("/w/subject", title: "Subject")
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

        #expect(
            store.releaseSections(
                tab: .pending,
                filterText: ""
            )
            .count == 1
        )
        #expect(
            store.selectableReadyRows(filterText: "")
                .map(\.candidateKey) == ["/w/subject"]
        )
    }

    @MainActor
    @Test("row lookup addresses candidates inside grouped sections")
    func rowLookupUsesSections() {
        #expect(
            seededStore().triageRow(forKey: "/w/collection/second")?
                .folderName == "Second"
        )
        #expect(seededStore().triageRow(forKey: "/w/missing") == nil)
    }

    @MainActor
    @Test("bulk skip keeps queue order and excludes ineligible or stale picks")
    func skippableSelectionUsesLiveQueueRows() {
        let keys = seededStore()
            .skippableCandidateKeys(
                in: [
                    "/w/ready-a",
                    "/w/ready-b",
                    "/w/collection/second",
                    "/w/done",
                    "/w/skipped",
                    "/w/missing",
                ]
            )

        #expect(
            keys
                == [
                    "/w/ready-b",
                    "/w/ready-a",
                    "/w/collection/second",
                ]
        )
    }

    @Test("section identity keeps path components distinct")
    func sectionIdentityDoesNotConcatenatePaths() {
        let first = ReleaseQueueSectionID(
            tab: .pending,
            watchedFolderPath: "/a|b",
            groupRelativeFolderPath: "c"
        )
        let second = ReleaseQueueSectionID(
            tab: .pending,
            watchedFolderPath: "/a",
            groupRelativeFolderPath: "b|c"
        )
        #expect(first != second)
    }
}

@Suite("ImportStore sidebar covers")
struct ImportStoreSidebarCoverTests {
    @MainActor
    @Test("the sidebar uses every user-chosen cover")
    func selectedCoversWinOverQueueThumbnail() throws {
        let store = ImportStore()
        let key = "/w/subject"
        var candidate = folderCandidate(
            folderPath: key,
            watchedFolderPath: "/w",
            name: "Subject"
        )
        let row = readyRow(
            key,
            title: "Subject",
            coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg"
        )

        let remoteCover = try #require(PreviewData.remoteCovers.last)
        let localArtwork = try #require(
            PreviewData.bridgeCandidateFiles.images.last
        )
        let choices = [
            remoteCover.coverChoice,
            try #require(localArtwork.coverChoice),
        ]
        for choice in choices {
            candidate.selectedCover = choice
            store.folderCandidates[key] = candidate

            #expect(
                store.sidebarCover(for: row)
                    == ImageContent(bridge: choice.thumbnailSource)
            )
        }
    }

    @MainActor
    @Test("the sidebar uses the automatically selected default cover")
    func defaultCoverWinsOverQueueThumbnail() throws {
        let store = ImportStore()
        let key = "/w/subject"
        var candidate = folderCandidate(
            folderPath: key,
            watchedFolderPath: "/w",
            name: "Subject"
        )
        candidate.setReleaseDetail(PreviewData.releaseDetailBridge)
        store.folderCandidates[key] = candidate

        let row = readyRow(
            key,
            title: "Subject",
            coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg"
        )

        let releaseDetail = try #require(candidate.releaseDetailBridge)
        let defaultCover = try #require(releaseDetail.defaultCover)
        #expect(
            store.sidebarCover(for: row)
                == ImageContent(bridge: defaultCover.thumbnailSource)
        )
    }

    @MainActor
    @Test(
        "the sidebar retains the queue thumbnail before a candidate has a cover"
    )
    func queueThumbnailRendersBeforeCoverResolution() {
        let row = readyRow(
            "/w/subject",
            title: "Subject",
            coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg"
        )

        #expect(
            ImportStore().sidebarCover(for: row)
                == .remote(url: "https://example.com/queue-thumbnail.jpg")
        )
    }
}
