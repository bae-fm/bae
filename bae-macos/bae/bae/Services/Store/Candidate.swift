import BaeKit
import Foundation

// MARK: - CandidateSource

/// Source-specific data for a candidate. Folder candidates carry the watched
/// folder they were scanned from; re-identify candidates carry the existing
/// library release id.
enum CandidateSource: Equatable {
    case folder(
        watchedFolderPath: String
    )
    case releaseReIdentify(releaseId: String)
}

// MARK: - CandidatePick

/// The decision made for a candidate: which release, which metadata source it
/// came from, and how far the claim on it reaches. Every re-pick — switching
/// back from the folder's own tags, re-reading the mapping under a changed
/// shape — sends these three back, so none of them is lost on the way.
struct CandidatePick: Equatable {
    let releaseId: String
    let source: BridgeMetadataSource
    /// The claim the user holds this release at. `exact` unless they lowered
    /// it; carried so a re-pick keeps it rather than resetting it.
    let claim: BridgeClaimLevel
}

// MARK: - ImportIdentity

/// What a folder is being read as. `release` covers the identify phase too:
/// there the open question is *which* release, not whether there is one.
enum ImportIdentity: Equatable {
    case release
    case unknown
}

// MARK: - SearchTab

enum SearchTab: Hashable {
    case general
    case catalogNumber
    case barcode
}

struct ReleaseLibraryStatusSubscriptionKey: Hashable {
    let source: BridgeMetadataSource
    let releaseId: String
    let sourceGroupId: String?
}

final class ReleaseLibraryStatusObservation: Equatable, @unchecked Sendable {
    let identity = UUID()
    private var subscription: (any LiveSubscriptionProtocol)?

    func install(_ subscription: any LiveSubscriptionProtocol) {
        precondition(self.subscription == nil)
        self.subscription = subscription
    }

    deinit {
        subscription?.cancel()
    }

    static func == (
        lhs: ReleaseLibraryStatusObservation,
        rhs: ReleaseLibraryStatusObservation
    ) -> Bool {
        lhs === rhs
    }
}

// MARK: - CandidateSearchState

/// Per-tab search state plus form fields and active tab/source.
struct CandidateSearchState: Equatable {
    /// Search results for one (tab, source) combination, grouped into
    /// release-group cards by core.
    struct TabResults: Equatable {
        var groups: [ReleaseGroup] = []
        var libraryStatusSubscriptionKeys:
            Set<ReleaseLibraryStatusSubscriptionKey> = []
        var hasSearched: Bool = false
        var isSearching: Bool = false
    }

    /// Results keyed by tab, then source. A missing key is a combination the
    /// user hasn't searched yet — identical to `TabResults()`'s initial state.
    private var resultsByTabSource:
        [SearchTab: [BridgeMetadataSource: TabResults]] = [:]

    var searchArtist: String = ""
    var searchAlbum: String = ""
    var searchCatalog: String = ""
    var searchBarcode: String = ""
    var activeTab: SearchTab = .general
    var activeSource: BridgeMetadataSource = .musicBrainz
    var showManualSearch: Bool = false

    func activeResults() -> TabResults {
        results(forTab: activeTab, source: activeSource)
    }

    func results(forTab tab: SearchTab, source: BridgeMetadataSource)
        -> TabResults
    {
        resultsByTabSource[tab]?[source] ?? TabResults()
    }

    mutating func setResults(
        _ state: TabResults,
        forTab tab: SearchTab,
        source: BridgeMetadataSource
    ) {
        resultsByTabSource[tab, default: [:]][source] = state
    }

    func libraryStatusSubscriptionKeys()
        -> Set<ReleaseLibraryStatusSubscriptionKey>
    {
        resultsByTabSource.values
            .flatMap { bySource in
                bySource.values.flatMap(\.libraryStatusSubscriptionKeys)
            }
            .reduce(into: []) { $0.insert($1) }
    }
}

// MARK: - Candidate

/// A scanned import candidate, including all dynamic state the UI needs while
/// the user works through identification and import.
struct Candidate: Equatable, Identifiable {
    let source: CandidateSource
    /// Stable key — the folder path. Used as the dictionary key.
    let key: String
    let displayName: String

    /// Dynamic — mutated by the import-candidate projection or by views.
    var files: BridgeCandidateFiles
    var identifyState: IdentifyState = .idle
    var importStatus: BridgeCandidateImportStatus?
    /// The picked release's display detail. The confirm pane reads its cover
    /// art, track counts, and library status from it. It never seeds the editor:
    /// it is the picker's shape, and it collapses the release's artists and
    /// positions for display.
    var releaseDetailBridge: BridgeReleaseDetail?
    /// What the pick claims and where its metadata came from, as bae-core
    /// derived it from the evidence that identified this candidate. Rendered by
    /// the confirm header; `nil` while identifying and for an Unknown import,
    /// which claims nothing.
    var claim: BridgeClaimLine?
    /// The release the user clicked, held from the click itself so the results
    /// row stays selected while the prefetch is in flight. The claim arrives
    /// with the prefetch and names the same release.
    ///
    /// It outlives a switch to Unknown, which is what makes switching back a
    /// re-pick of this release rather than a trip through the search.
    var pick: CandidatePick?
    var libraryStatuses: [String: BridgeLibraryStatus] = [:]
    var libraryStatusSubscriptions:
        [ReleaseLibraryStatusSubscriptionKey: ReleaseLibraryStatusObservation] =
            [:]
    /// What the folder is being read as, set the moment the user says so —
    /// before the pick or the tag read it starts has come back.
    var identity: ImportIdentity = .release
    var error: String?
    /// The cover the *user* picked, `nil` while they have picked none. Only a
    /// pick crosses to the commit: with none, bae-core lands the release's own
    /// first cover option — the one ``coverFace`` is already showing — so an
    /// untouched pane must not send that option back as if it had been chosen.
    var coverPick: BridgeCoverChoice?

    /// The cover the pane shows: the user's pick, else the release's own first
    /// option, which is what an untouched commit lands.
    var coverFace: BridgeCoverChoice? {
        coverPick ?? releaseDetailBridge?.defaultCover
    }
    var search: CandidateSearchState = .init()
    /// Extracted signals (disc ID, barcodes, classified text), `nil` until the
    /// first extraction snapshot. The search UI surfaces these and feeds the
    /// `text` pools into the autocomplete fields.
    var signals: Signals?
    /// The interactive signals toolbar — the pre-shaped badge list core
    /// broadcasts alongside each identify-state transition. Empty until the
    /// first transition. The toolbar view iterates and renders it.
    var signalsToolbar: BridgeSignalsToolbar = BridgeSignalsToolbar(signals: [])
    /// The identity claim the import command carries: `claim.choice` for a
    /// source-backed pick, `.unknown` for an "Add as Unknown" import. The
    /// commit pipeline post-processes the seeded identity from it (Exact
    /// preserves `source_release_id`, Approximate NULLs it). `nil` while the
    /// user is still in the identify phase.
    var identityChoice: BridgeIdentityChoice?
    /// The album-level half of the editable metadata, seeded from the prefetch's
    /// editor seed (already masked for the claim by bae-core) or from the
    /// folder's own tags. Its `tracks` stay empty: the tracklist lives in
    /// `mapping`, where the row that produces a track is the row that edits it.
    var editValues: BridgeRawReleaseEdit?
    /// The picked release's own pressing fields — what claiming this pressing
    /// exactly is a claim about. `nil` for an Unknown import, which claims
    /// nothing. Editing `editValues` away from these is a different claim, and
    /// core is what says so.
    var exactPressing: BridgeRawPressingEdit?
    /// Every source unit the folder offers with the track committing makes of
    /// it. `nil` until the folder's mapping has been read. Excluding a file or
    /// dropping a row trims it in place — re-reading it from core would throw
    /// away the user's edits.
    var mapping: BridgeMappingTable?

    // periphery:ignore
    /// In-flight search task. Replacing it cancels the old one via the
    /// previous wrapper's `deinit`; clearing it after the task completes
    /// lets the wrapper drop without re-cancelling. Removing the candidate
    /// from the store drops the wrapper too, cancelling the request.
    var searchTask: CancelOnDeinit?
    // periphery:ignore
    /// In-flight prefetch task. Same pattern as `searchTask`.
    var prefetchTask: CancelOnDeinit?

    var id: String {
        key
    }

    init(bridge: BridgeFolderCandidate) {
        source = .folder(
            watchedFolderPath: bridge.watchedFolderPath
        )
        key = bridge.folderPath
        displayName = bridge.sourceFolderName
        files = bridge.files
    }

    func withSessionState(from existing: Candidate) -> Candidate {
        var copy = self
        copy.identifyState = existing.identifyState
        copy.importStatus = existing.importStatus
        copy.releaseDetailBridge = existing.releaseDetailBridge
        copy.claim = existing.claim
        copy.pick = existing.pick
        copy.libraryStatuses = existing.libraryStatuses
        copy.libraryStatusSubscriptions = existing.libraryStatusSubscriptions
        copy.identity = existing.identity
        copy.error = existing.error
        copy.coverPick = existing.coverPick
        copy.search = existing.search
        copy.signals = existing.signals
        copy.signalsToolbar = existing.signalsToolbar
        copy.identityChoice = existing.identityChoice
        copy.editValues = existing.editValues
        copy.exactPressing = existing.exactPressing
        copy.mapping = existing.mapping
        copy.searchTask = existing.searchTask
        copy.prefetchTask = existing.prefetchTask
        return copy
    }

    /// Construct a re-identify candidate. The release already lives in
    /// the library; identify-pipeline events stream into this candidate the
    /// same way folder events do, so the existing `ImportSearchPane` UI renders
    /// unchanged.
    init(
        reIdentifyKey: String,
        releaseId: String,
        displayName: String
    ) {
        source = .releaseReIdentify(releaseId: releaseId)
        key = reIdentifyKey
        self.displayName = displayName
        // Re-identify candidates read their files from the DB, not the
        // scanner's scan-event channel, so they start with an empty set.
        files = BridgeCandidateFiles(
            files: [],
            formatLabel: "",
            collapsedDirectories: []
        )
    }

    /// What committing this candidate writes: the album fields the editor
    /// holds, over the tracklist the mapping table's rows are. `nil` until
    /// something has been settled for this folder — a release, or its own tags
    /// — and again after a re-pick fails, which unsettles it: there is no
    /// identity to commit under then, so there is nothing to commit.
    var commitEdit: BridgeRawReleaseEdit? {
        guard identityChoice != nil, let editValues, let mapping else {
            return nil
        }
        return BridgeRawReleaseEdit(
            albumTitle: editValues.albumTitle,
            albumArtistText: editValues.albumArtistText,
            pressing: editValues.pressing,
            tracks: mapping.commitTracks
        )
    }

    /// The watched folder this candidate was scanned from — the candidate-list
    /// group it belongs to. `nil` for re-identify candidates (not grouped).
    var watchedFolderPath: String? {
        if case .folder(let watchedFolderPath) = source {
            return watchedFolderPath
        }
        return nil
    }

}
