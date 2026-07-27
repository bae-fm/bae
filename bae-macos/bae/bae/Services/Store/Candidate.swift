import BaeKit
import Foundation

// MARK: - CandidateSource

/// Source-specific data for a candidate. Folder candidates carry their release
/// folder path plus the watched folder they were scanned from (the candidate
/// list groups by it); re-identify candidates carry the existing library
/// release id — the identify pipeline reads its files from the DB rather than
/// scanning a folder. Created by the import-candidate projection (folder) or
/// by `AlbumDetailView`'s "Re-identify..." action.
enum CandidateSource: Equatable {
    case folder(folderPath: String, watchedFolderPath: String)
    case releaseReIdentify(releaseId: String)
}

// MARK: - CandidateMode

enum CandidateMode: Equatable {
    case identifying
    case loadingDetail
    case confirming
}

// MARK: - SearchTab

enum SearchTab: Hashable {
    case general
    case catalogNumber
    case barcode
}

// MARK: - CandidateSearchState

/// Per-tab search state plus form fields and active tab/source.
struct CandidateSearchState: Equatable {
    /// Search results for one (tab, source) combination, grouped into
    /// release-group cards by core.
    struct TabResults: Equatable {
        var groups: [ReleaseGroup] = []
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
}

// MARK: - Candidate

/// A scanned import candidate, including all dynamic state the UI needs while
/// the user works through identification and import.
struct Candidate: Equatable, Identifiable {
    let source: CandidateSource
    /// Stable key — the folder path. Used as the dictionary key.
    let key: String
    let displayName: String
    /// Track count for the candidate. Folder candidates always have a value.
    let trackCount: UInt32?

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
    var pickedReleaseId: String?
    var libraryStatuses: [String: BridgeLibraryStatus] = [:]
    var mode: CandidateMode = .identifying
    var error: String?
    var selectedCover: BridgeCoverChoice?
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
    /// Raw editable metadata form shown on the confirmation page. Seeded from
    /// the prefetch's editor seed, which bae-core has already masked for the
    /// claim; bae-core shapes the raw form back into the wire edit committed as
    /// the metadata overlay alongside the import command.
    var editValues: BridgeRawReleaseEdit?

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
            folderPath: bridge.folderPath,
            watchedFolderPath: bridge.watchedFolderPath
        )
        key = bridge.folderPath
        displayName = bridge.sourceFolderName
        trackCount = bridge.trackCount
        files = bridge.files
    }

    func withSessionState(from existing: Candidate) -> Candidate {
        var copy = self
        copy.identifyState = existing.identifyState
        copy.importStatus = existing.importStatus
        copy.releaseDetailBridge = existing.releaseDetailBridge
        copy.claim = existing.claim
        copy.pickedReleaseId = existing.pickedReleaseId
        copy.libraryStatuses = existing.libraryStatuses
        copy.mode = existing.mode
        copy.error = existing.error
        copy.selectedCover = existing.selectedCover
        copy.search = existing.search
        copy.signals = existing.signals
        copy.signalsToolbar = existing.signalsToolbar
        copy.identityChoice = existing.identityChoice
        copy.editValues = existing.editValues
        copy.searchTask = existing.searchTask
        copy.prefetchTask = existing.prefetchTask
        return copy
    }

    /// Construct a re-identify candidate. The release already lives in
    /// the library; identify-pipeline events stream into this candidate the
    /// same way folder events do, so the existing `ImportSearchPane` UI renders
    /// unchanged. `trackCount` is the number of tracks on the release — used by
    /// the barcode lookup phase to match expected track counts in candidate
    /// releases.
    init(
        reIdentifyKey: String,
        releaseId: String,
        displayName: String,
        trackCount: UInt32
    ) {
        source = .releaseReIdentify(releaseId: releaseId)
        key = reIdentifyKey
        self.displayName = displayName
        self.trackCount = trackCount
        // Re-identify candidates read their files from the DB, not the
        // scanner's scan-event channel, so they start with an empty set.
        files = BridgeCandidateFiles(files: [], formatLabel: "")
    }

    /// The watched folder this candidate was scanned from — the candidate-list
    /// group it belongs to. `nil` for re-identify candidates (not grouped).
    var watchedFolderPath: String? {
        if case .folder(_, let watchedFolderPath) = source {
            return watchedFolderPath
        }
        return nil
    }

    // MARK: - Mutating helpers

    /// Drop cached library statuses matching `shouldRemove` — both the
    /// search-merged top-level map and the copies embedded in terminal
    /// identify states. An absent status renders as "not in your library",
    /// so removal is the invalidation.
    mutating func removeLibraryStatuses(
        where shouldRemove: (String, BridgeLibraryStatus) -> Bool
    ) {
        libraryStatuses = libraryStatuses.filter {
            !shouldRemove($0.key, $0.value)
        }
        identifyState.removeLibraryStatuses(where: shouldRemove)
    }
}
