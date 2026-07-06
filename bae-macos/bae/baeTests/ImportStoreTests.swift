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
    @MainActor
    @Test("release invalidation clears a matching importStatus")
    func releaseInvalidationClearsImportStatus() throws {
        let store = ImportStore()
        var candidate = makeCandidate("c1")
        candidate.importStatus = .complete(albumId: "al-1", releaseId: "rel-1")
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
        candidate.importStatus = .complete(albumId: "al-2", releaseId: "rel-2")
        store.folderCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

        #expect(
            store.folderCandidates["c1"]?.importStatus
                == .complete(albumId: "al-2", releaseId: "rel-2")
        )
    }

    @MainActor
    @Test("release invalidation clears the candidate that imported it")
    func releaseInvalidationClearsByReleaseId() throws {
        let store = ImportStore()
        var imported = makeCandidate("c1")
        imported.importStatus = .complete(albumId: "al-1", releaseId: "rel-1")
        store.folderCandidates["c1"] = imported
        var sibling = makeCandidate("c2")
        sibling.importStatus = .complete(albumId: "al-1", releaseId: "rel-2")
        store.folderCandidates["c2"] = sibling

        store.removeLibraryStatus(releaseId: "rel-1")

        let cleared = try #require(store.folderCandidates["c1"])
        #expect(cleared.importStatus == nil)
        let unaffected = try #require(store.folderCandidates["c2"])
        #expect(
            unaffected.importStatus
                == .complete(albumId: "al-1", releaseId: "rel-2")
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
            discidSourceLabel: nil,
            matchedBarcode: nil,
            trackCount: 10
        )
        store.reIdentifyCandidates["c1"] = candidate

        store.removeLibraryStatus(releaseId: "rel-1")

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
