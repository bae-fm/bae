import BaeKit
import Foundation
import Testing

@testable import bae

// MARK: - Bridge snapshot builders

/// Hand-built bridge records for the snapshot reducers.

private func emptyBridgeFiles() -> BridgeCandidateFiles {
    BridgeCandidateFiles(
        fileMetadataIdentity: "empty-audio-files",
        files: [],
        coverFiles: [],
        sourceAudio: nil
    )
}

private func bridgeFolder(
    folderPath: Data,
    watchedFolderPath: Data,
    name: String,
    skipped: Bool = false
) -> BridgeFolderCandidate {
    BridgeFolderCandidate(
        parts: [],
        candidateKey: folderPath,
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        files: emptyBridgeFiles(),
        trackCount: 10,
        skipped: skipped,
    )
}

// MARK: - Triage row builders

/// An identified Pending row for the folder at `path`: linked, no import
/// status. Shared with the paged list's tests.
func identifiedRow(
    _ path: String,
    title: String,
    metadataSummary: BridgeTriageMetadataSummary? = nil,
    cover: BridgeCoverImageSource? = nil
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: Data(path.utf8),
        folderName: title,
        watchedFolderPath: Data("/w".utf8),
        actionable: true,
        placement: .pending,
        live: BridgeCandidateLiveState(
            identifying: nil,
            import: nil,
            actions: [],
            standing: .notLookedUp,
            badge: nil
        ),
        metadataSummary: metadataSummary,
        cover: cover,
        importStatus: nil,
        upload: nil,
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(catalog: .musicBrainz, key: "rel-\(path)")
        ),
        reading: metadataSummary == nil
            ? .unidentified
            : .identified(records: [
                BridgeReleaseRecord(
                    catalog: .musicBrainz,
                    url: "https://musicbrainz.org/release/rel-\(path)"
                )
            ])
    )
}

private func detail(
    folderPath: Data,
    watchedFolderPath: Data,
    name: String,
    skipped: Bool = false,
    cover: BridgeCoverChoice? = nil,
    presentation: BridgeMetadataPresentation = .draft
) -> BridgeImportCandidateDetail {
    BridgeImportCandidateDetail(
        candidate: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped
        ),
        partCount: 0,
        actionable: true,
        placement: .pending(folderCheck: nil, records: []),
        live: BridgeCandidateLiveState(
            identifying: nil,
            import: nil,
            actions: [
                .import, .identify, .resetToFileMetadata, .clearMetadata,
                .skip,
            ],
            standing: .notLookedUp,
            badge: nil
        ),
        importStatus: nil,
        signals: nil,
        stage: .drafting(
            draft: BridgeCandidateDraftPane(
                pickedLibraryStatus: nil,
                album: .new,
                metadataDraft: MappingFixtures.albumEdit,
                albumArtistStanding: nil,
                metadataDraftIsBlank: false,
                metadataProvenance: MappingFixtures.provenance,
                releaseLink: MappingFixtures.releaseLink,
                mapping: BridgeMappingTable(
                    images: [],
                    trackSides: [],
                    files: [],
                    untitledTrackCount: 0
                ),
                cover: cover,
                failure: nil,
                paneFailure: nil,
                opensOn: presentation
            )
        )
    )
}

@Suite("ImportStore per-candidate reads")
struct ImportStoreCandidateDetailTests {
    @MainActor
    @Test("a read installs the folder and its row")
    func installsTheRead() throws {
        let store = ImportStore()

        store.applyCandidateDetail(
            key: Data("/w1/a".utf8),
            detail: detail(
                folderPath: Data("/w1/a".utf8),
                watchedFolderPath: Data("/w1".utf8),
                name: "A"
            )
        )

        let read = try #require(store.selectedCandidates[Data("/w1/a".utf8)])
        #expect(read.displayName == "A")
        #expect(read.placement == .pending(folderCheck: nil, records: []))
        #expect(read.importStatus == nil)
    }

    @MainActor
    @Test("a re-read replaces the candidate with what the read says")
    func reReadReplacesCandidate() throws {
        let store = ImportStore()
        let existing = Candidate(
            detail: detail(
                folderPath: Data("/w1/a".utf8),
                watchedFolderPath: Data("/w1".utf8),
                name: "A"
            )
        )
        store.selectedCandidates[Data("/w1/a".utf8)] = existing

        // Same key, renamed and skipped, and where the pane opens moved on.
        store.applyCandidateDetail(
            key: Data("/w1/a".utf8),
            detail: detail(
                folderPath: Data("/w1/a".utf8),
                watchedFolderPath: Data("/w1".utf8),
                name: "A-renamed",
                skipped: true,
                presentation: .findOnline
            )
        )

        let merged = try #require(store.selectedCandidates[Data("/w1/a".utf8)])
        // A pane nobody moved stands where the read says it opens.
        #expect(merged.metadataPresentation == .findOnline)
        // Scan fields come from the incoming read.
        #expect(merged.displayName == "A-renamed")
        #expect(merged.files.files.isEmpty)
    }

    /// A stored pick that asks nothing moves a pane nobody moved from Find
    /// online to the draft.
    @MainActor
    @Test("a read that opens the pane on the draft moves it there")
    func aReadOnTheDraftMovesThePane() throws {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: Data("/w1/a".utf8),
            detail: detail(
                folderPath: Data("/w1/a".utf8),
                watchedFolderPath: Data("/w1".utf8),
                name: "A",
                presentation: .findOnline
            )
        )
        #expect(
            try #require(store.selectedCandidates[Data("/w1/a".utf8)])
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: Data("/w1/a".utf8),
            detail: detail(
                folderPath: Data("/w1/a".utf8),
                watchedFolderPath: Data("/w1".utf8),
                name: "A",
                presentation: .draft
            )
        )

        #expect(
            try #require(store.selectedCandidates[Data("/w1/a".utf8)])
                .metadataPresentation == .draft
        )
    }

    /// Once the person moves the pane, it stays where they put it, and the
    /// text typed in its search form stays, whatever the next read says;
    /// showing another candidate forgets both.
    @MainActor
    @Test("the pane keeps where the person moved it until it shows another")
    func thePaneKeepsWhereThePersonMovedIt() throws {
        let store = ImportStore()
        let key = Data("/w1/a".utf8)
        let read = {
            detail(
                folderPath: key,
                watchedFolderPath: Data("/w1".utf8),
                name: "A",
                presentation: .draft
            )
        }
        store.applyCandidateDetail(key: key, detail: read())
        store.movePane(.search, forKey: key)
        var typed = CandidateSearchState()
        typed.searchArtist = "Artist Name"
        store.commitSearchForm(typed, forKey: key)

        store.applyCandidateDetail(key: key, detail: read())

        let shown = try #require(store.selectedCandidates[key])
        #expect(
            shown.panePosition
                == CandidatePanePosition(
                    presentation: .findOnline,
                    findOnlineSection: .search
                )
        )
        #expect(shown.search == typed)

        store.selectedCandidates.removeValue(forKey: key)
        store.applyCandidateDetail(key: key, detail: read())
        let reopened = try #require(store.selectedCandidates[key])
        #expect(reopened.metadataPresentation == .draft)
        #expect(reopened.search == CandidateSearchState())
    }

    /// A move writes what it names and reads nothing core sent: the pane
    /// lands in the same place whichever surface the read opens it on.
    @MainActor
    @Test("a pane move lands the same whatever the read opens on")
    func aPaneMoveLandsTheSameWhateverTheReadOpensOn() throws {
        for opensOn in [BridgeMetadataPresentation.draft, .findOnline] {
            let store = ImportStore()
            let key = Data("/w1/a".utf8)
            store.applyCandidateDetail(
                key: key,
                detail: detail(
                    folderPath: key,
                    watchedFolderPath: Data("/w1".utf8),
                    name: "A",
                    presentation: opensOn
                )
            )

            store.movePane(.openSection(.search), forKey: key)
            store.movePane(.back, forKey: key)
            let back = try #require(store.selectedCandidates[key])
            #expect(
                back.panePosition
                    == CandidatePanePosition(
                        presentation: .draft,
                        findOnlineSection: .search
                    )
            )

            store.movePane(.findOnline, forKey: key)
            let reopened = try #require(store.selectedCandidates[key])
            #expect(
                reopened.panePosition
                    == CandidatePanePosition(
                        presentation: .findOnline,
                        findOnlineSection: .search
                    )
            )
        }
    }
}

@Suite("ImportStore sidebar covers")
struct ImportStoreSidebarCoverTests {
    @MainActor
    @Test("applied row covers remain after deselection")
    func appliedCoversSurviveDeselection() throws {
        let store = ImportStore()
        let path = "/w/subject"
        let key = Data(path.utf8)
        let remoteCover = try #require(PreviewData.remoteCovers.last)
        let localChoice = BridgeCoverChoice(
            selection: .releaseImage(fileId: "cover.jpg"),
            image: .local(file: PreviewData.previewArtFile("cover.jpg"))
        )
        let choices = [remoteCover.coverChoice, localChoice]
        for choice in choices {
            let row = identifiedRow(
                path,
                title: "Subject",
                metadataSummary: BridgeTriageMetadataSummary(
                    albumTitle: "Applied Draft",
                    albumArtists: BridgeCreditLine(names: [])
                ),
                cover: choice.image
            )
            store.applyCandidateDetail(
                key: key,
                detail: detail(
                    folderPath: key,
                    watchedFolderPath: Data("/w".utf8),
                    name: "Subject",
                    cover: choice
                )
            )
            store.selectedCandidates.removeValue(forKey: key)

            #expect(
                store.sidebarCover(for: row)
                    == ImageContent(bridge: choice.image)
            )
        }
    }
}

@MainActor
@Suite("ImportStore queue summary")
struct ImportStoreSummaryTests {
    /// A queue not read yet is not a queue without watched folders: the import
    /// screen would show its add-a-folder page to a library that has some.
    @Test("the watched folders are unknown until the summary lands")
    func watchedFoldersAreUnknownBeforeTheSummary() {
        let store = ImportStore()
        #expect(store.summary == nil)
        #expect(store.hasWatchedFolders == nil)

        store.applySummary(ImportListHosting.settledSummary)

        #expect(store.hasWatchedFolders == false)
    }
}
