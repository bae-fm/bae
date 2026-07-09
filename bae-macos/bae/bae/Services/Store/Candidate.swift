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

    var generalMb = TabResults()
    var generalDiscogs = TabResults()
    var catalogMb = TabResults()
    var catalogDiscogs = TabResults()
    var barcodeMb = TabResults()
    var barcodeDiscogs = TabResults()

    var searchArtist: String = ""
    var searchAlbum: String = ""
    var searchCatalog: String = ""
    var searchBarcode: String = ""
    var activeTab: SearchTab = .general
    var activeSource: MetadataSource = .musicBrainz
    var showManualSearch: Bool = false

    func activeResults() -> TabResults {
        results(forTab: activeTab, source: activeSource)
    }

    func results(forTab tab: SearchTab, source: MetadataSource) -> TabResults {
        switch (tab, source) {
        case (.general, .musicBrainz): generalMb
        case (.general, .discogs): generalDiscogs
        case (.catalogNumber, .musicBrainz): catalogMb
        case (.catalogNumber, .discogs): catalogDiscogs
        case (.barcode, .musicBrainz): barcodeMb
        case (.barcode, .discogs): barcodeDiscogs
        }
    }

    mutating func setResults(
        _ state: TabResults,
        forTab tab: SearchTab,
        source: MetadataSource
    ) {
        switch (tab, source) {
        case (.general, .musicBrainz): generalMb = state
        case (.general, .discogs): generalDiscogs = state
        case (.catalogNumber, .musicBrainz): catalogMb = state
        case (.catalogNumber, .discogs): catalogDiscogs = state
        case (.barcode, .musicBrainz): barcodeMb = state
        case (.barcode, .discogs): barcodeDiscogs = state
        }
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
    var files: CandidateFiles
    var identifyState: IdentifyState = .idle
    var importStatus: ImportStatus?
    /// Whether the user manually skipped this candidate. Drives the import
    /// view's Skipped tab; flipped by the import-candidate projection once the
    /// skip toggle round-trips through core.
    var skipped: Bool = false
    /// Whether this candidate's file structure was already imported (matched by
    /// content hash). Set from the scan; drives the Added tab so an
    /// already-imported folder surfaces as added even without a fresh import in
    /// the session.
    var isAdded: Bool = false
    /// Full release detail fetched after the user picks a pressing. Stored as
    /// the bridge type (not just the `ImportReleaseDetail` projection) so the
    /// bottom pane can re-seed the editor when the user flips the Exact /
    /// Metadata-only choice — re-shaping needs the source detail, not only the
    /// cover/track-count projection below.
    var releaseDetailBridge: BridgeReleaseDetail?
    /// Cover-art / track-count projection of the picked release, derived from
    /// `releaseDetailBridge` for the confirm pane.
    var releaseDetail: ImportReleaseDetail? {
        releaseDetailBridge.map(ImportReleaseDetail.init(bridge:))
    }
    var libraryStatuses: [String: LibraryStatus] = [:]
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
    var signalsToolbar: SignalsToolbar = SignalsToolbar(signals: [])
    /// User's identity claim picked at result-row time. Carried
    /// forward through the prefetch + confirmation flow into the import
    /// command so the commit pipeline can post-process the seeded
    /// identity (Exact preserves `source_release_id`, Approximate NULLs
    /// it). `nil` while the user is still in the identify phase.
    var identityChoice: IdentityChoice?
    /// Raw editable metadata form shown on the confirmation page. Seeded
    /// from `releaseDetail` after prefetch and the user's identity
    /// choice; bae-core shapes the raw form into the wire edit committed
    /// as the metadata overlay alongside the import command.
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
        files = CandidateFiles(bridge: bridge.files)
        skipped = bridge.skipped
        isAdded = bridge.isAdded
    }

    func withSessionState(from existing: Candidate) -> Candidate {
        var copy = self
        copy.identifyState = existing.identifyState
        copy.importStatus = existing.importStatus
        copy.releaseDetailBridge = existing.releaseDetailBridge
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
        files = .empty
    }

    /// Folder path if this is a folder candidate; nil otherwise.
    /// Used for "Reveal in Finder" affordances.
    var folderPathIfReal: String? {
        if case .folder(let path, _) = source {
            return path
        }
        return nil
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
        where shouldRemove: (String, LibraryStatus) -> Bool
    ) {
        libraryStatuses = libraryStatuses.filter {
            !shouldRemove($0.key, $0.value)
        }
        identifyState.removeLibraryStatuses(where: shouldRemove)
    }
}
