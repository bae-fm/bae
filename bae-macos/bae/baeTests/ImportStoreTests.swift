import BaeKit
import Foundation
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

/// Hand-built bridge records for the snapshot reducers.

private func emptyBridgeFiles() -> BridgeCandidateFiles {
    BridgeCandidateFiles(
        fileMetadataIdentity: "empty-audio-files",
        files: [],
        coverFiles: [],
        sourceAudio: nil
    )
}

private func bridgeFiles(fileMetadataIdentity: String) -> BridgeCandidateFiles {
    BridgeCandidateFiles(
        fileMetadataIdentity: fileMetadataIdentity,
        files: [
            BridgeCandidateFile(
                file: BridgeFileInfo(
                    name: "01.flac",
                    size: 100,
                    dirPrefix: nil,
                    fileName: "01.flac",
                    localPath: "/music/01.flac",
                    audioFormat: nil
                ),
                role: .audio
            )
        ],
        coverFiles: [],
        sourceAudio: nil
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
        parts: [],
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        files: emptyBridgeFiles(),
        trackCount: trackCount,
        skipped: skipped,
        isAdded: isAdded
    )
}

/// A folder `Candidate` with the given scan flags.
private func folderCandidate(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    isAdded: Bool = false
) -> Candidate {
    Candidate(
        bridge: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped,
            isAdded: isAdded
        )
    )
}

private func bridgeInvalid(
    folderPath: String,
    watchedFolderPath: String,
    name: String
) -> BridgeInvalidCandidate {
    BridgeInvalidCandidate(
        candidateKey: folderPath,
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        displayPath: name,
        separable: false,
        reason: .noValidAudio
    )
}

// MARK: - Triage row builders

private func matchedRelease(
    releaseId: String,
    title: String,
    trackCount: UInt32? = 10,
    cover: BridgeRemoteImageSet? = nil
) -> BridgeMatchedRelease {
    BridgeMatchedRelease(
        releaseId: releaseId,
        title: title,
        artist: "Artist",
        pressing: trackCount.map {
            BridgeMatchedPressing(
                year: 2000,
                media: PreviewData.media(.cd),
                trackCount: $0
            )
        },
        cover: cover,
        evidence: BridgeMatchEvidence(source: .musicBrainz, signal: .discId)
    )
}

/// An identified Pending row: matched, no import status. Shared with the
/// paged list's tests.
func identifiedRow(
    _ key: String,
    title: String,
    matchedCover: BridgeRemoteImageSet? = nil,
    metadataSummary: BridgeTriageMetadataSummary? = nil,
    cover: BridgeCoverImageSource? = nil
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        actionable: true,
        placement: .pending,
        live: BridgeCandidateLiveState(
            identification: nil,
            import: nil,
            actions: [],
            standing: .notLookedUp
        ),
        matched: matchedRelease(
            releaseId: "rel-\(key)",
            title: title,
            cover: matchedCover
        ),
        metadataSummary: metadataSummary,
        cover: cover,
        importStatus: nil,
        metadataProvenance: .externalRelease(
            record: BridgeMetadataRef(catalog: .musicBrainz, key: "rel-\(key)")
        ),
        reading: metadataSummary == nil
            ? .unidentified
            : .identified(records: [
                BridgeReleaseRecord(
                    catalog: .musicBrainz,
                    url: "https://musicbrainz.org/release/rel-\(key)"
                )
            ]),
        selected: false
    )
}

private func skippedRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        actionable: true,
        placement: .skipped,
        live: BridgeCandidateLiveState(
            identification: nil,
            import: nil,
            actions: [],
            standing: .notLookedUp
        ),
        matched: nil,
        metadataSummary: nil,
        cover: nil,
        importStatus: nil,
        metadataProvenance: nil,
        reading: .unidentified,
        selected: false
    )
}

private func detail(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    resumedIdentifyState: BridgeIdentifyState = .idle,
    cover: BridgeCoverChoice? = nil,
    release: BridgeReleaseDetail? = nil,
    presentation: BridgeMetadataPresentation = .draft
) -> BridgeImportCandidateDetail {
    BridgeImportCandidateDetail(
        candidate: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped
        ),
        actionable: true,
        resumedIdentifyState: resumedIdentifyState,
        placement: .pending(folderCheck: nil, records: []),
        live: BridgeCandidateLiveState(
            identification: nil,
            import: nil,
            actions: [
                .import, .identify, .resetToFileMetadata, .clearMetadata,
                .skip,
            ],
            standing: .notLookedUp
        ),
        importStatus: nil,
        release: release,
        pickedLibraryStatus: nil,
        fileEvidence: [],
        metadataDraft: MappingFixtures.albumEdit,
        artistResolutions: [],
        metadataDraftIsBlank: false,
        metadataProvenance: MappingFixtures.provenance,
        releaseLink: MappingFixtures.releaseLink,
        metadataAuthor: .person,
        metadataRevision: 1,
        mapping: BridgeMappingTable(
            images: [],
            trackSections: [],
            files: []
        ),
        cover: cover,
        signals: nil,
        failure: nil,
        session: MappingFixtures.session(presentation: presentation)
    )
}

@Suite("ImportStore per-candidate reads")
struct ImportStoreCandidateDetailTests {
    @MainActor
    @Test("a read installs the folder, its resumed state and its row")
    func installsTheRead() throws {
        let store = ImportStore()

        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A",
                resumedIdentifyState: .notFoundAnywhere(run: nil)
            )
        )

        let read = try #require(store.selectedCandidates["/w1/a"])
        #expect(read.displayName == "A")
        // With no run live the resumed state is what the pane shows.
        #expect(read.resumedIdentifyState == .notFoundAnywhere(run: nil))
        #expect(read.placement == .pending(folderCheck: nil, records: []))
        #expect(read.importStatus == nil)
    }

    @MainActor
    @Test("a re-read keeps the editor state on its key")
    func keepsEditorState() throws {
        let store = ImportStore()
        var existing = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        existing.libraryStatuses = ["rel-1": makeStatus(albumId: "al-1")]
        store.selectedCandidates["/w1/a"] = existing

        // Same key, renamed and skipped, and the pane's stored session moved on.
        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A-renamed",
                skipped: true,
                presentation: .findOnline
            )
        )

        let merged = try #require(store.selectedCandidates["/w1/a"])
        // The pane's in-memory work survives the re-read.
        #expect(merged.libraryStatuses["rel-1"] != nil)
        // The pane's session is the candidate's, so it comes with the read.
        #expect(merged.metadataPresentation == .findOnline)
        // Scan fields come from the incoming read.
        #expect(merged.displayName == "A-renamed")
        #expect(merged.files.files.isEmpty)
    }

    /// A stored pick that asks nothing moves the open pane from Find online to
    /// the draft.
    @MainActor
    @Test("a read that stores the pane on the draft moves it there")
    func aReadOnTheDraftMovesThePane() throws {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A",
                presentation: .findOnline
            )
        )
        #expect(
            try #require(store.selectedCandidates["/w1/a"])
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A",
                presentation: .draft
            )
        )

        #expect(
            try #require(store.selectedCandidates["/w1/a"])
                .metadataPresentation == .draft
        )
    }
}

@Suite("ImportStore sidebar covers")
struct ImportStoreSidebarCoverTests {
    @MainActor
    @Test("applied row covers remain after deselection")
    func appliedCoversSurviveDeselection() throws {
        let store = ImportStore()
        let key = "/w/subject"
        let remoteCover = try #require(PreviewData.remoteCovers.last)
        let localArtwork = try #require(
            PreviewData.bridgeCandidateFiles.images.last
        )
        let choices = [
            remoteCover.coverChoice,
            try #require(localArtwork.coverChoice),
        ]
        for choice in choices {
            let row = identifiedRow(
                key,
                title: "Subject",
                matchedCover: BridgeRemoteImageSet(
                    url: "https://example.com/queue-cover.jpg",
                    downscaled: []
                ),
                metadataSummary: BridgeTriageMetadataSummary(
                    albumTitle: "Applied Draft",
                    albumArtistAssignments: []
                ),
                cover: choice.image
            )
            store.applyCandidateDetail(
                key: key,
                detail: detail(
                    folderPath: key,
                    watchedFolderPath: "/w",
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

    @Test("the sidebar renders only the cover resolved by core")
    func sidebarDoesNotDeriveACoverFromMatchMetadata() {
        let row = identifiedRow(
            "/w/subject",
            title: "Subject",
            matchedCover: BridgeRemoteImageSet(
                url: "https://example.com/queue-cover.jpg",
                downscaled: []
            )
        )

        #expect(ImportStore().sidebarCover(for: row) == nil)
    }
}
