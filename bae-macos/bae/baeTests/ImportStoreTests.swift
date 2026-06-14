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

private func makeStatus(albumId: String) -> LibraryStatus {
    LibraryStatus(
        bridge: BridgeLibraryStatus(
            releaseId: "unused",
            releaseInLibrary: true,
            albumInLibrary: true,
            albumTitle: "Album Title",
            albumId: albumId
        )
    )
}

@Suite("ImportStore library-removal invalidation")
struct ImportStoreRemovalTests {
    /// Deleting the imported album must clear the candidate's terminal
    /// "Imported / View in Library" state — it points at a dead album and
    /// locks the candidate against re-import.
    @MainActor
    @Test("album removal clears a matching importStatus")
    func albumRemovalClearsImportStatus() throws {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.importStatus = .complete(albumId: "al-1", releaseId: "rel-1")
        store.folderCandidates["c1"] = candidate

        store.handleAlbumRemoved(albumId: "al-1", releaseIds: ["rel-1"])

        let survived = try #require(store.folderCandidates["c1"])
        #expect(survived.importStatus == nil)
    }

    @MainActor
    @Test("album removal leaves an unrelated importStatus alone")
    func albumRemovalKeepsUnrelatedImportStatus() {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.importStatus = .complete(albumId: "al-2", releaseId: "rel-2")
        store.folderCandidates["c1"] = candidate

        store.handleAlbumRemoved(albumId: "al-1", releaseIds: ["rel-1"])

        #expect(
            store.folderCandidates["c1"]?.importStatus
                == .complete(albumId: "al-2", releaseId: "rel-2")
        )
    }

    /// A release deleted out of a surviving album: the candidate that
    /// imported that release loses its terminal state; siblings keep theirs.
    @MainActor
    @Test("release removal clears the candidate that imported it")
    func releaseRemovalClearsByReleaseId() throws {
        let store = ImportStore()
        var imported = makeCandidate("c1")
        imported.importStatus = .complete(albumId: "al-1", releaseId: "rel-1")
        store.folderCandidates["c1"] = imported
        var sibling = makeCandidate("c2")
        sibling.importStatus = .complete(albumId: "al-1", releaseId: "rel-2")
        store.folderCandidates["c2"] = sibling

        store.handleReleaseRemoved(releaseId: "rel-1")

        let cleared = try #require(store.folderCandidates["c1"])
        #expect(cleared.importStatus == nil)
        let unaffected = try #require(store.folderCandidates["c2"])
        #expect(
            unaffected.importStatus
                == .complete(albumId: "al-1", releaseId: "rel-2")
        )
    }

    @MainActor
    @Test("album removal drops search-merged library statuses")
    func albumRemovalDropsLibraryStatuses() throws {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.libraryStatuses = [
            "rel-1": makeStatus(albumId: "al-1"),
            // A sibling pressing of the removed album, never imported itself:
            // its status points at the removed album and is equally stale.
            "rel-sibling": makeStatus(albumId: "al-1"),
            "rel-other": makeStatus(albumId: "al-other"),
        ]
        store.folderCandidates["c1"] = candidate

        store.handleAlbumRemoved(albumId: "al-1", releaseIds: ["rel-1"])

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
            matches: [],
            libraryStatuses: [
                "rel-1": makeStatus(albumId: "al-1"),
                "rel-other": makeStatus(albumId: "al-other"),
            ],
            trackCount: 10,
            group: GroupKey(
                bridge: BridgeGroupKey(
                    source: .musicBrainz,
                    sourceGroupId: "rg-1",
                    sourceLabel: "MusicBrainz",
                    groupUrl: ""
                )
            ),
            source: .discid,
            provenance: []
        )
        store.reIdentifyCandidates["c1"] = candidate

        store.handleReleaseRemoved(releaseId: "rel-1")

        guard
            case .found(_, let statuses, _, _, _, _) =
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
            discidSourceLabel: nil,
            matchedBarcode: nil,
            trackCount: 10
        )
        store.reIdentifyCandidates["c1"] = candidate

        store.handleReleaseRemoved(releaseId: "rel-1")

        guard
            case .conflict(_, let discid, _, let barcode, _, _, _) =
                store.reIdentifyCandidates["c1"]?.identifyState
        else {
            Issue.record("identify state should remain .conflict")
            return
        }
        #expect(discid.isEmpty)
        #expect(barcode.isEmpty)
    }
}
