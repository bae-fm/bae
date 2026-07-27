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
        reason: .noValidAudio
    )
}

// MARK: - Triage row builders

private func matchedRelease(
    releaseId: String,
    title: String,
    trackCount: UInt32? = 10
) -> BridgeMatchedRelease {
    BridgeMatchedRelease(
        releaseId: releaseId,
        title: title,
        artist: "Artist",
        pressing: trackCount.map {
            BridgeMatchedPressing(year: 2000, format: "CD", trackCount: $0)
        },
        coverThumbnailUrl: nil,
        evidence: BridgeMatchEvidence(source: .musicBrainz, signal: .discId),
        // A lone disc-ID match, so core's rule claims the pressing.
        claim: .exact(releaseId: releaseId, source: .musicBrainz)
    )
}

/// A Ready row: matched, selectable, no import status.
private func readyRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        placement: .ready,
        matched: matchedRelease(releaseId: "rel-\(key)", title: title),
        selectable: true,
        importStatus: nil
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
        placement: .needsYou(group: group, reason: reason),
        matched: nil,
        selectable: false,
        importStatus: nil
    )
}

private func doneRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        placement: .done,
        matched: matchedRelease(releaseId: "rel-\(key)", title: title),
        selectable: false,
        importStatus: .complete(releaseId: "rel-\(key)", albumId: "al-\(key)")
    )
}

private func skippedRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        placement: .skipped,
        matched: nil,
        selectable: false,
        importStatus: nil
    )
}

private func emptyTriageQueue() -> BridgeTriageQueue {
    BridgeTriageQueue(
        rows: [],
        invalid: [],
        counts: BridgeTriageTabCounts(
            ready: 0,
            needsYou: 0,
            done: 0,
            skipped: 0
        )
    )
}

@Suite("ImportStore library-removal invalidation")
struct ImportStoreRemovalTests {
    @MainActor
    @Test("release invalidation clears a matching importStatus")
    func releaseInvalidationClearsImportStatus() throws {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.importStatus = .complete(releaseId: "rel-1", albumId: "al-1")
        store.folderCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

        let survived = try #require(store.folderCandidates["c1"])
        #expect(survived.importStatus == nil)
    }

    @MainActor
    @Test("release invalidation leaves an unrelated importStatus alone")
    func releaseInvalidationKeepsUnrelatedImportStatus() {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.importStatus = .complete(releaseId: "rel-2", albumId: "al-2")
        store.folderCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

        #expect(
            store.folderCandidates["c1"]?.importStatus
                == .complete(releaseId: "rel-2", albumId: "al-2")
        )
    }

    @MainActor
    @Test("release invalidation clears the candidate that imported it")
    func releaseInvalidationClearsByReleaseId() throws {
        let store = ImportStore()
        var imported = makeCandidate("c1")
        imported.importStatus = .complete(releaseId: "rel-1", albumId: "al-1")
        store.folderCandidates["c1"] = imported
        var sibling = makeCandidate("c2")
        sibling.importStatus = .complete(releaseId: "rel-2", albumId: "al-1")
        store.folderCandidates["c2"] = sibling

        store.removeLibraryStatus(releaseId: "rel-1")

        let cleared = try #require(store.folderCandidates["c1"])
        #expect(cleared.importStatus == nil)
        let unaffected = try #require(store.folderCandidates["c2"])
        #expect(
            unaffected.importStatus
                == .complete(releaseId: "rel-2", albumId: "al-1")
        )
    }

    @MainActor
    @Test("album deletion invalidates each removed release status")
    func albumDeletionInvalidatesEachRemovedReleaseStatus() throws {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.libraryStatuses = [
            "rel-1": makeStatus(albumId: "al-1"),
            "rel-sibling": makeStatus(albumId: "al-1"),
            "rel-other": makeStatus(albumId: "al-other"),
        ]
        store.folderCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")
        store.removeLibraryStatus(releaseId: "rel-sibling")

        let survived = try #require(store.folderCandidates["c1"])
        #expect(survived.libraryStatuses["rel-1"] == nil)
        #expect(survived.libraryStatuses["rel-sibling"] == nil)
        #expect(survived.libraryStatuses["rel-other"] != nil)
    }

    @MainActor
    @Test("release removal drops statuses embedded in a found identify state")
    func releaseRemovalSweepsIdentifyState() {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.identifyState = .found(
            group: ReleaseGroup(
                bridge: BridgeReleaseGroup(
                    id: "group-1",
                    title: "Album Title",
                    artist: "Artist Name",
                    coverArt: nil,
                    sourceLabel: "MusicBrainz",
                    groupUrl: nil,
                    yearMin: nil,
                    yearMax: nil,
                    pressings: []
                )
            ),
            libraryStatuses: [
                "rel-1": makeStatus(albumId: "al-1"),
                "rel-other": makeStatus(albumId: "al-other"),
            ],
            trackCount: 10,
            provenance: [:]
        )
        store.reIdentifyCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

        guard
            case .found(_, let statuses, _, _) =
                store.reIdentifyCandidates["c1"]?.identifyState
        else {
            Issue.record("identify state should remain .found")
            return
        }
        #expect(statuses["rel-1"] == nil)
        #expect(statuses["rel-other"] != nil)
    }

    @MainActor
    @Test("release removal sweeps both sides of a conflict identify state")
    func releaseRemovalSweepsConflictState() {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.identifyState = .conflict(
            discidResults: [],
            discidLibraryStatuses: ["rel-1": makeStatus(albumId: "al-1")],
            barcodeResults: [],
            barcodeLibraryStatuses: ["rel-1": makeStatus(albumId: "al-1")],
            matchedBarcode: nil,
            trackCount: 10
        )
        store.reIdentifyCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

        guard
            case .conflict(_, let discid, _, let barcode, _, _) =
                store.reIdentifyCandidates["c1"]?.identifyState
        else {
            Issue.record("identify state should remain .conflict")
            return
        }
        #expect(discid.isEmpty)
        #expect(barcode.isEmpty)
    }
}

@Suite("ImportStore.applyImportCandidatesSnapshot")
struct ImportStoreBatchSnapshotTests {
    @MainActor
    @Test("inserts fresh candidates and watched folders")
    func insertsFresh() throws {
        let store = ImportStore()

        store.applyImportCandidatesSnapshot(
            BridgeImportCandidatesSnapshot(
                watchedFolders: [BridgeWatchedFolder(path: "/w1", name: "w1")],
                folderCandidates: [
                    BridgeFolderImportCandidateSnapshot(
                        candidate: bridgeFolder(
                            folderPath: "/w1/a",
                            watchedFolderPath: "/w1",
                            name: "A"
                        ),
                        runtime: idleRuntime(
                            importStatus: .complete(
                                releaseId: "rel-1",
                                albumId: "al-1"
                            )
                        )
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
        // A fresh candidate (no prior session state) takes its runtime from the
        // snapshot.
        #expect(
            fresh.importStatus == .complete(releaseId: "rel-1", albumId: "al-1")
        )
    }

    @MainActor
    @Test("carries session state forward, taking scan fields from the snapshot")
    func carriesSessionState() throws {
        let store = ImportStore()
        var existing = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        existing.mode = .confirming
        existing.importStatus = .complete(releaseId: "rel-1", albumId: "al-1")
        existing.libraryStatuses = ["rel-1": makeStatus(albumId: "al-1")]
        store.folderCandidates["/w1/a"] = existing

        // Same key, renamed + skip flipped, and an idle runtime with no status.
        store.applyImportCandidatesSnapshot(
            BridgeImportCandidatesSnapshot(
                watchedFolders: [BridgeWatchedFolder(path: "/w1", name: "w1")],
                folderCandidates: [
                    BridgeFolderImportCandidateSnapshot(
                        candidate: bridgeFolder(
                            folderPath: "/w1/a",
                            watchedFolderPath: "/w1",
                            name: "A-renamed",
                            skipped: true
                        ),
                        runtime: idleRuntime()
                    )
                ],
                invalidCandidates: []
            )
        )

        let merged = try #require(store.folderCandidates["/w1/a"])
        // Session state carried from the existing candidate — the batch path
        // discards the snapshot's idle runtime in favor of it.
        #expect(merged.mode == .confirming)
        #expect(
            merged.importStatus
                == .complete(releaseId: "rel-1", albumId: "al-1")
        )
        #expect(merged.libraryStatuses["rel-1"] != nil)
        // Scan fields come from the incoming snapshot.
        #expect(merged.displayName == "A-renamed")
    }

    @MainActor
    @Test("drops candidates absent from the new snapshot")
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
            BridgeImportCandidatesSnapshot(
                watchedFolders: [BridgeWatchedFolder(path: "/w1", name: "w1")],
                folderCandidates: [
                    BridgeFolderImportCandidateSnapshot(
                        candidate: bridgeFolder(
                            folderPath: "/w1/a",
                            watchedFolderPath: "/w1",
                            name: "A"
                        ),
                        runtime: idleRuntime()
                    )
                ],
                invalidCandidates: []
            )
        )

        #expect(store.folderCandidates["/w1/a"] != nil)
        #expect(store.folderCandidates["/w1/b"] == nil)
    }
}

@Suite("ImportStore.applyImportCandidateSnapshot")
struct ImportStoreSingleSnapshotTests {
    @MainActor
    @Test("a nil snapshot removes the key")
    func nilRemoves() {
        let store = ImportStore()
        store.folderCandidates["/w1/a"] = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )

        store.applyImportCandidateSnapshot(key: "/w1/a", snapshot: nil)

        #expect(store.folderCandidates["/w1/a"] == nil)
    }

    @MainActor
    @Test(".folder merges session state and applies the runtime")
    func folderMergesAndAppliesRuntime() throws {
        let store = ImportStore()
        var existing = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        existing.mode = .confirming
        existing.importStatus = .complete(
            releaseId: "rel-old",
            albumId: "al-old"
        )
        store.folderCandidates["/w1/a"] = existing

        store.applyImportCandidateSnapshot(
            key: "/w1/a",
            snapshot: .folder(
                candidate: bridgeFolder(
                    folderPath: "/w1/a",
                    watchedFolderPath: "/w1",
                    name: "A"
                ),
                runtimeSnapshot: idleRuntime(
                    importStatus: .complete(
                        releaseId: "rel-new",
                        albumId: "al-new"
                    )
                )
            )
        )

        let merged = try #require(store.folderCandidates["/w1/a"])
        // Session field carried from the existing candidate...
        #expect(merged.mode == .confirming)
        // ...while the single-candidate path applies the fresh runtime on top,
        // so the runtime's import status wins over the carried one.
        #expect(
            merged.importStatus
                == .complete(releaseId: "rel-new", albumId: "al-new")
        )
    }

    @MainActor
    @Test(".invalid drops the key from folderCandidates")
    func invalidDropsFolderCandidate() {
        let store = ImportStore()
        store.folderCandidates["/w1/a"] = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )

        store.applyImportCandidateSnapshot(
            key: "/w1/a",
            snapshot: .invalid(
                candidate: bridgeInvalid(
                    folderPath: "/w1/a",
                    watchedFolderPath: "/w1",
                    name: "A"
                )
            )
        )

        // The sidebar now reads invalid folders from `triageQueue.invalid`,
        // read separately through the triage projection — this path only has
        // to stop treating the key as an addressable candidate.
        #expect(store.folderCandidates["/w1/a"] == nil)
    }

    @MainActor
    @Test(".runtime routes to a folder candidate and copies its fields")
    func runtimeRoutesToFolder() throws {
        let store = ImportStore()
        store.folderCandidates["/w1/a"] = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )

        store.applyImportCandidateSnapshot(
            key: "/w1/a",
            snapshot: .runtime(
                key: "/w1/a",
                runtimeSnapshot: BridgeCandidateRuntimeSnapshot(
                    identifyState: .triangulating(
                        discid: .computing,
                        barcode: .scanning
                    ),
                    signalsToolbar: BridgeSignalsToolbar(signals: []),
                    signals: nil,
                    importStatus: .complete(releaseId: "rel-1", albumId: "al-1")
                )
            )
        )

        let candidate = try #require(store.folderCandidates["/w1/a"])
        #expect(
            candidate.identifyState
                == .triangulating(discid: .computing, barcode: .scanning)
        )
        #expect(
            candidate.importStatus
                == .complete(releaseId: "rel-1", albumId: "al-1")
        )
    }

    @MainActor
    @Test(".runtime with a reidentify: key routes to a re-identify candidate")
    func runtimeRoutesToReIdentify() throws {
        let store = ImportStore()
        store.reIdentifyCandidates["reidentify:r1"] =
            makeCandidate("reidentify:r1")

        store.applyImportCandidateSnapshot(
            key: "reidentify:r1",
            snapshot: .runtime(
                key: "reidentify:r1",
                runtimeSnapshot: idleRuntime(
                    importStatus: .complete(releaseId: "rel-1", albumId: "al-1")
                )
            )
        )

        #expect(
            store.reIdentifyCandidates["reidentify:r1"]?.importStatus
                == .complete(releaseId: "rel-1", albumId: "al-1")
        )
        #expect(store.folderCandidates["reidentify:r1"] == nil)
    }
}

// MARK: - Triage rendering

@Suite("ImportStore triage rendering")
struct ImportStoreTriageRenderingTests {
    /// One row in each tab, and every Needs-you group represented at least
    /// once (two in "pick a pressing" so sort order is observable).
    private func seededStore() -> ImportStore {
        let store = ImportStore()
        store.triageQueue = BridgeTriageQueue(
            rows: [
                readyRow("/w/ready-b", title: "Beta"),
                readyRow("/w/ready-a", title: "Alpha"),
                needsYouRow(
                    "/w/pick-1",
                    title: "Pick One",
                    group: .pickAPressing,
                    reason: .disagreement(
                        disagreement: .severalMatches(count: 3)
                    )
                ),
                needsYouRow(
                    "/w/pick-2",
                    title: "Pick Two",
                    group: .pickAPressing,
                    reason: .disagreement(
                        disagreement: .severalMatches(count: 2)
                    )
                ),
                needsYouRow(
                    "/w/conflict",
                    title: "Conflicted",
                    group: .signalsDisagree,
                    reason: .disagreement(disagreement: .signalsConflict)
                ),
                needsYouRow(
                    "/w/pending",
                    title: "Pending",
                    group: .stillIdentifying,
                    reason: .stillIdentifying(phase: .running)
                ),
                doneRow("/w/done", title: "Finished"),
                skippedRow("/w/skipped", title: "Set Aside"),
            ],
            invalid: [
                bridgeInvalid(
                    folderPath: "/w/bad",
                    watchedFolderPath: "/w",
                    name: "Bad Folder"
                )
            ],
            counts: BridgeTriageTabCounts(
                ready: 2,
                needsYou: 4,
                done: 1,
                skipped: 2
            )
        )
        return store
    }

    @MainActor
    @Test("rows(tab:) scopes to the tab and sorts A-Z")
    func rowsScopesAndSorts() {
        let rows = seededStore()
            .rows(
                tab: .ready,
                filterText: "",
                sortOrder: .nameAZ
            )
        #expect(rows.map(\.folderName) == ["Alpha", "Beta"])
    }

    @MainActor
    @Test("rows(tab:) sorts Z-A")
    func rowsSortsDescending() {
        let rows = seededStore()
            .rows(
                tab: .ready,
                filterText: "",
                sortOrder: .nameZA
            )
        #expect(rows.map(\.folderName) == ["Beta", "Alpha"])
    }

    @MainActor
    @Test("rows(tab:) filters by display title and folder name")
    func rowsFilters() {
        let rows = seededStore()
            .rows(
                tab: .ready,
                filterText: "alpha",
                sortOrder: .nameAZ
            )
        #expect(rows.map(\.folderName) == ["Alpha"])
    }

    @MainActor
    @Test(
        "checkboxes exist only where selectable — every Ready row, no other tab's"
    )
    func selectableIsReadyOnly() {
        let store = seededStore()
        let ready = store.rows(tab: .ready, filterText: "", sortOrder: .nameAZ)
        #expect(!ready.isEmpty)
        #expect(ready.allSatisfy { $0.selectable })

        let done = store.rows(tab: .done, filterText: "", sortOrder: .nameAZ)
        let skipped = store.skippedRows(filterText: "", sortOrder: .nameAZ)
        let needsYou = store.needsYouGroups(filterText: "", sortOrder: .nameAZ)
            .flatMap(\.rows)
        #expect(!(done + needsYou).contains { $0.selectable })
        #expect(
            !skipped.contains {
                if case .candidate(let row) = $0 { return row.selectable }
                return false
            }
        )
    }

    @MainActor
    @Test("needsYouGroups orders groups per core and drops empty ones")
    func needsYouGroupsOrdered() {
        let groups = seededStore()
            .needsYouGroups(
                filterText: "",
                sortOrder: .nameAZ
            )
        // Core declares PickAPressing, SignalsDisagree, ..., StillIdentifying
        // in that order; only the three represented in the fixture appear.
        #expect(
            groups.map(\.group) == [
                .pickAPressing, .signalsDisagree, .stillIdentifying,
            ]
        )
        #expect(
            groups.first { $0.group == .pickAPressing }?.rows.map(\.folderName)
                == ["Pick One", "Pick Two"]
        )
    }

    @MainActor
    @Test("needsYouGroups filter can empty a group out entirely")
    func needsYouGroupsFilterDropsEmptyGroup() {
        let groups = seededStore()
            .needsYouGroups(
                filterText: "conflicted",
                sortOrder: .nameAZ
            )
        #expect(groups.map(\.group) == [.signalsDisagree])
    }

    @MainActor
    @Test("skippedRows merges skipped candidates and invalid folders")
    func skippedRowsMerge() {
        let rows = seededStore()
            .skippedRows(
                filterText: "",
                sortOrder: .nameAZ
            )
        #expect(rows.map(\.id) == ["/w/bad", "/w/skipped"])
    }

    @MainActor
    @Test("skippedRows filters candidate and invalid rows by the same text")
    func skippedRowsFilter() {
        let rows = seededStore()
            .skippedRows(
                filterText: "bad",
                sortOrder: .nameAZ
            )
        #expect(rows.map(\.id) == ["/w/bad"])
    }

    /// Plan test 7: answering a Needs-you row moves it out of that tab. The
    /// store has no reducer of its own for this — a fresh `triageQueue`
    /// (what the projection assigns after core reclassifies the row) is the
    /// whole mechanism, so the test drives exactly that.
    @MainActor
    @Test(
        "answering a Needs-you row moves it into Ready and out of every group"
    )
    func answeringMovesTheRowOut() {
        let store = ImportStore()
        store.triageQueue = BridgeTriageQueue(
            rows: [
                needsYouRow(
                    "/w/subject",
                    title: "Subject",
                    group: .pickAPressing,
                    reason: .disagreement(
                        disagreement: .severalMatches(count: 2)
                    )
                )
            ],
            invalid: [],
            counts: BridgeTriageTabCounts(
                ready: 0,
                needsYou: 1,
                done: 0,
                skipped: 0
            )
        )

        #expect(
            store.needsYouGroups(filterText: "", sortOrder: .nameAZ)
                .flatMap(\.rows).map(\.candidateKey) == ["/w/subject"]
        )
        #expect(
            store.rows(tab: .ready, filterText: "", sortOrder: .nameAZ)
                .isEmpty
        )

        // The user picked a pressing; the next triage-queue read (what the
        // projection would apply) reclassifies the row to Ready.
        store.triageQueue = BridgeTriageQueue(
            rows: [readyRow("/w/subject", title: "Subject")],
            invalid: [],
            counts: BridgeTriageTabCounts(
                ready: 1,
                needsYou: 0,
                done: 0,
                skipped: 0
            )
        )

        #expect(
            store.needsYouGroups(filterText: "", sortOrder: .nameAZ).isEmpty
        )
        #expect(
            store.rows(tab: .ready, filterText: "", sortOrder: .nameAZ)
                .map(\.candidateKey) == ["/w/subject"]
        )
    }

    @MainActor
    @Test("an empty triage queue renders no rows in any tab")
    func emptyQueueIsEmptyEverywhere() {
        let store = ImportStore()
        store.triageQueue = emptyTriageQueue()
        #expect(
            store.rows(tab: .ready, filterText: "", sortOrder: .nameAZ)
                .isEmpty
        )
        #expect(
            store.needsYouGroups(filterText: "", sortOrder: .nameAZ).isEmpty
        )
        #expect(
            store.rows(tab: .done, filterText: "", sortOrder: .nameAZ)
                .isEmpty
        )
        #expect(store.skippedRows(filterText: "", sortOrder: .nameAZ).isEmpty)
    }
}
