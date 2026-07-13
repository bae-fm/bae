import BaeKit
import Testing

@testable import bae

private func makeCandidate(_ key: String) -> Candidate {
    Candidate(
        reIdentifyKey: key,
        releaseId: "existing-release",
        displayName: "Candidate \(key)",
        trackCount: 10
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
    BridgeCandidateFiles(
        audio: .trackFiles(files: []),
        artwork: [],
        documents: []
    )
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
/// import status, for the tab-assignment and grouping suites.
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
            source: .discid,
            provenance: [:]
        )
        store.reIdentifyCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

        guard
            case .found(_, let statuses, _, _, _) =
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
    @Test("inserts fresh candidates, watched folders, and invalids")
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
        #expect(store.invalidCandidates["/w1/bad"] != nil)
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
        #expect(merged.skipped)
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
    @Test("a nil snapshot removes the key from both dicts")
    func nilRemoves() {
        let store = ImportStore()
        store.folderCandidates["/w1/a"] = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        store.invalidCandidates["/w1/bad"] = bridgeInvalid(
            folderPath: "/w1/bad",
            watchedFolderPath: "/w1",
            name: "Bad"
        )

        store.applyImportCandidateSnapshot(key: "/w1/a", snapshot: nil)
        store.applyImportCandidateSnapshot(key: "/w1/bad", snapshot: nil)

        #expect(store.folderCandidates["/w1/a"] == nil)
        #expect(store.invalidCandidates["/w1/bad"] == nil)
    }

    @MainActor
    @Test(".folder merges session state, applies the runtime, clears invalid")
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
        store.invalidCandidates["/w1/a"] = bridgeInvalid(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )

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
        #expect(store.invalidCandidates["/w1/a"] == nil)
    }

    @MainActor
    @Test(".invalid moves the folder candidate into invalid")
    func invalidMoves() {
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

        #expect(store.folderCandidates["/w1/a"] == nil)
        #expect(store.invalidCandidates["/w1/a"] != nil)
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

@Suite("ImportStore.tab")
struct ImportStoreTabTests {
    @MainActor
    @Test("skipped wins over a completed import")
    func skippedWins() {
        let store = ImportStore()
        let candidate = folderCandidate(
            folderPath: "/w/a",
            watchedFolderPath: "/w",
            name: "A",
            skipped: true,
            importStatus: .complete(releaseId: "rel", albumId: "al")
        )
        #expect(store.tab(for: candidate) == .skipped)
    }

    @MainActor
    @Test("a completed import is Added")
    func completeIsAdded() {
        let store = ImportStore()
        let candidate = folderCandidate(
            folderPath: "/w/a",
            watchedFolderPath: "/w",
            name: "A",
            importStatus: .complete(releaseId: "rel", albumId: "al")
        )
        #expect(store.tab(for: candidate) == .added)
    }

    @MainActor
    @Test("an already-imported folder is Added without a fresh import")
    func alreadyImportedIsAdded() {
        let store = ImportStore()
        let candidate = folderCandidate(
            folderPath: "/w/a",
            watchedFolderPath: "/w",
            name: "A",
            isAdded: true
        )
        #expect(store.tab(for: candidate) == .added)
    }

    @MainActor
    @Test("a plain candidate is New")
    func plainIsNew() {
        let store = ImportStore()
        let candidate = folderCandidate(
            folderPath: "/w/a",
            watchedFolderPath: "/w",
            name: "A"
        )
        #expect(store.tab(for: candidate) == .new)
    }
}

@Suite("ImportStore grouping")
struct ImportStoreGroupingTests {
    /// Two watched folders. `/w1` holds two New candidates (inserted Beta then
    /// Alpha, so date order is observable) plus one Added; `/w2` holds one
    /// skipped candidate and one invalid folder.
    @MainActor
    private func seededStore() -> ImportStore {
        let store = ImportStore()
        store.watchedFolders = [
            BridgeWatchedFolder(path: "/w1", name: "w1"),
            BridgeWatchedFolder(path: "/w2", name: "w2"),
        ]
        store.folderCandidates["/w1/beta"] = folderCandidate(
            folderPath: "/w1/beta",
            watchedFolderPath: "/w1",
            name: "Beta"
        )
        store.folderCandidates["/w1/alpha"] = folderCandidate(
            folderPath: "/w1/alpha",
            watchedFolderPath: "/w1",
            name: "Alpha"
        )
        store.folderCandidates["/w1/done"] = folderCandidate(
            folderPath: "/w1/done",
            watchedFolderPath: "/w1",
            name: "Done",
            importStatus: .complete(releaseId: "rel", albumId: "al")
        )
        store.folderCandidates["/w2/gamma"] = folderCandidate(
            folderPath: "/w2/gamma",
            watchedFolderPath: "/w2",
            name: "Gamma",
            skipped: true
        )
        store.invalidCandidates["/w2/bad"] = bridgeInvalid(
            folderPath: "/w2/bad",
            watchedFolderPath: "/w2",
            name: "BadFolder"
        )
        return store
    }

    @MainActor
    @Test("candidateGroups keeps every watched folder and sorts A–Z")
    func groupsSortAscending() {
        let groups = seededStore()
            .candidateGroups(
                tab: .new,
                filterText: "",
                sortOrder: .nameAZ
            )
        #expect(groups.map(\.folder.path) == ["/w1", "/w2"])
        #expect(groups[0].candidates.map(\.displayName) == ["Alpha", "Beta"])
        // The other folder has no New candidates but still appears when unfiltered.
        #expect(groups[1].candidates.isEmpty)
    }

    @MainActor
    @Test("candidateGroups sorts Z–A")
    func groupsSortDescending() {
        let groups = seededStore()
            .candidateGroups(
                tab: .new,
                filterText: "",
                sortOrder: .nameZA
            )
        #expect(groups[0].candidates.map(\.displayName) == ["Beta", "Alpha"])
    }

    @MainActor
    @Test("candidateGroups date order is insertion order, oldest reverses it")
    func groupsSortByDate() {
        let store = seededStore()
        // Inserted Beta before Alpha.
        #expect(
            store.candidateGroups(
                tab: .new,
                filterText: "",
                sortOrder: .dateAddedNewest
            )[0]
            .candidates.map(\.displayName) == ["Beta", "Alpha"]
        )
        #expect(
            store.candidateGroups(
                tab: .new,
                filterText: "",
                sortOrder: .dateAddedOldest
            )[0]
            .candidates.map(\.displayName) == ["Alpha", "Beta"]
        )
    }

    @MainActor
    @Test("candidateGroups filter drops folders with no match")
    func groupsFilterDropsEmptyFolders() {
        let groups = seededStore()
            .candidateGroups(
                tab: .new,
                filterText: "alph",
                sortOrder: .nameAZ
            )
        #expect(groups.map(\.folder.path) == ["/w1"])
        #expect(groups[0].candidates.map(\.displayName) == ["Alpha"])
    }

    @MainActor
    @Test("candidateTabCounts counts every tab, invalids under Skipped")
    func tabCounts() {
        let counts = seededStore().candidateTabCounts()
        #expect(counts.new == 2)
        #expect(counts.added == 1)
        // One skipped candidate plus one invalid folder.
        #expect(counts.skipped == 2)
    }

    @MainActor
    @Test("skippedGroups merges skipped candidates and invalid folders")
    func skippedGroupsMergeRows() {
        let groups = seededStore()
            .skippedGroups(
                filterText: "",
                sortOrder: .nameAZ
            )
        // /w1 has no skipped rows and drops out.
        #expect(groups.map(\.folder.path) == ["/w2"])
        #expect(groups[0].rows.map(\.id) == ["/w2/gamma", "/w2/bad"])
    }

    @MainActor
    @Test("skippedGroups filters candidate and invalid rows by text")
    func skippedGroupsFilter() {
        let groups = seededStore()
            .skippedGroups(
                filterText: "bad",
                sortOrder: .nameAZ
            )
        // Only the invalid folder matches "bad"; the skipped candidate drops.
        #expect(groups.map(\.folder.path) == ["/w2"])
        #expect(groups[0].rows.map(\.id) == ["/w2/bad"])
    }
}
