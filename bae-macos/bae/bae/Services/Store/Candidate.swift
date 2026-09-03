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

// MARK: - SearchTab

enum SearchTab: Hashable {
    case general
    case catalogNumber
    case barcode
}

struct SearchSourceSelection: Hashable {
    let musicBrainz: Bool
    let discogs: Bool

    var bridgeSources: BridgeSearchSources? {
        switch (musicBrainz, discogs) {
        case (true, true): .both
        case (true, false): .one(source: .musicBrainz)
        case (false, true): .one(source: .discogs)
        case (false, false): nil
        }
    }
}

struct CandidateSearchSlot: Hashable {
    let tab: SearchTab
    let sources: SearchSourceSelection
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

enum CandidateMetadataPresentation: Equatable {
    case draft
    case findOnline
    case fileTags
}

/// One metadata application from its click until both the bridge command and
/// authoritative candidate-detail delivery have confirmed it.
final class CandidateMetadataApplicationSession: Equatable,
    @unchecked Sendable
{
    let provenance: BridgeMetadataProvenance

    private(set) var commandRevision: UInt64?
    private(set) var deliveredRevision: UInt64?
    private var task: Task<Void, Never>?
    private var onConfirmed: (() -> Void)?

    init(
        provenance: BridgeMetadataProvenance,
        onConfirmed: (() -> Void)? = nil
    ) {
        self.provenance = provenance
        self.onConfirmed = onConfirmed
    }

    func install(_ task: Task<Void, Never>) {
        precondition(self.task == nil)
        self.task = task
    }

    func recordCommandSuccess(revision: UInt64) {
        commandRevision = revision
    }

    func recordDetailDelivery(revision: UInt64) {
        deliveredRevision = revision
    }

    var isConfirmed: Bool {
        commandRevision != nil && commandRevision == deliveredRevision
    }

    func takeConfirmation() -> (() -> Void)? {
        precondition(isConfirmed)
        defer { onConfirmed = nil }
        return onConfirmed
    }

    deinit {
        task?.cancel()
    }

    static func == (
        lhs: CandidateMetadataApplicationSession,
        rhs: CandidateMetadataApplicationSession
    ) -> Bool {
        lhs === rhs
    }
}

final class CandidateFileTagsPreviewSession: Equatable, @unchecked Sendable {
    private var task: Task<Void, Never>?

    func install(_ task: Task<Void, Never>) {
        precondition(self.task == nil)
        self.task = task
    }

    deinit {
        task?.cancel()
    }

    static func == (
        lhs: CandidateFileTagsPreviewSession,
        rhs: CandidateFileTagsPreviewSession
    ) -> Bool {
        lhs === rhs
    }
}

enum CandidateFileTagsPreviewState: Equatable {
    case unloaded
    case loading(CandidateFileTagsPreviewSession)
    case loaded(BridgeReleaseUserEdit)
    case failed

    var isLoading: Bool {
        if case .loading = self { return true }
        return false
    }

    var edit: BridgeReleaseUserEdit? {
        guard case .loaded(let edit) = self else { return nil }
        return edit
    }
}

// MARK: - CandidateSearchState

/// Per-tab search state plus form fields and the selected providers.
struct CandidateSearchState: Equatable {
    /// Search results for one (tab, provider selection) combination, grouped into
    /// release-group cards by core.
    struct TabResults: Equatable {
        var groups: [ReleaseGroup] = []
        var libraryStatusSubscriptionKeys:
            Set<ReleaseLibraryStatusSubscriptionKey> = []
        var hasSearched: Bool = false
        var isSearching: Bool = false
    }

    /// Results keyed by tab, then provider selection. A missing key is a combination the
    /// user hasn't searched yet — identical to `TabResults()`'s initial state.
    private var resultsByTabSources:
        [SearchTab: [SearchSourceSelection: TabResults]] = [:]

    var searchArtist: String = ""
    var searchAlbum: String = ""
    var searchCatalog: String = ""
    var searchBarcode: String = ""
    var activeTab: SearchTab = .general
    var musicBrainzSelected: Bool = true
    var discogsSelected: Bool = true

    func selectedSources(discogsAvailable: Bool) -> SearchSourceSelection {
        SearchSourceSelection(
            musicBrainz: musicBrainzSelected,
            discogs: discogsAvailable && discogsSelected
        )
    }

    func activeResults(discogsAvailable: Bool) -> TabResults {
        results(for: activeSlot(discogsAvailable: discogsAvailable))
    }

    func activeSlot(discogsAvailable: Bool) -> CandidateSearchSlot {
        CandidateSearchSlot(
            tab: activeTab,
            sources: selectedSources(discogsAvailable: discogsAvailable)
        )
    }

    func results(for slot: CandidateSearchSlot) -> TabResults {
        resultsByTabSources[slot.tab]?[slot.sources] ?? TabResults()
    }

    mutating func setResults(_ state: TabResults, for slot: CandidateSearchSlot)
    {
        resultsByTabSources[slot.tab, default: [:]][slot.sources] = state
    }

    func libraryStatusSubscriptionKeys()
        -> Set<ReleaseLibraryStatusSubscriptionKey>
    {
        resultsByTabSources.values
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
    /// The state the candidate's stored verdict stands back up as, from the
    /// candidate list. `.idle` when nothing is stored for its current files.
    /// What a surface shows is this or the run in flight — see
    /// `shownIdentifyState(resumed:runtime:)`, which reads the run from the
    /// candidate-runtime signal rather than from here.
    var resumedIdentifyState: IdentifyState = .idle
    /// Everything the pane draws, as core reads it back for this key: the
    /// metadata draft and provenance, mapping table, cover, and the
    /// last failed import. `nil` until the per-candidate
    /// read has answered, and for a re-identify session, which has no folder.
    ///
    /// The pane keeps no copy of any of it. A control writes through the
    /// importer, core commits, and the next value of this lands here.
    var detail: BridgeImportCandidateDetail?
    /// How the sidebar places this candidate — the same row the list holds,
    /// read by key alongside the folder. `nil` for a re-identify session,
    /// which has no scanned folder and so no row.
    var row: BridgeTriageRow?
    var libraryStatuses: [String: BridgeLibraryStatus] = [:]
    var libraryStatusSubscriptions:
        [ReleaseLibraryStatusSubscriptionKey: ReleaseLibraryStatusObservation] =
            [:]
    /// The last command this pane ran, when it failed — a selection whose read
    /// dropped, a write that would not land, a commit the fields do not
    /// support. Shown in the banner and cleared by the next command.
    var error: String?
    /// The metadata source currently being selected. A release row uses its
    /// release ID for selection feedback while the pane keeps showing stored
    /// data.
    var metadataApplicationSession: CandidateMetadataApplicationSession?
    /// The draft or temporary source browser occupying the metadata slot.
    /// Browsing never replaces the stored draft; applying a result does.
    var metadataPresentation: CandidateMetadataPresentation = .draft
    /// The lazy File Tags read for this candidate. It is session state rather
    /// than candidate detail: reading tags does not choose that seed.
    var fileTagsPreview: CandidateFileTagsPreviewState = .unloaded

    var provenanceInFlight: BridgeMetadataProvenance? {
        metadataApplicationSession?.provenance
    }

    var loadingReleaseId: String? {
        guard
            case .externalRelease(_, let releaseId) = provenanceInFlight
        else {
            return nil
        }
        return releaseId
    }
    var search: CandidateSearchState = .init()
    // periphery:ignore
    /// In-flight search task. Replacing it cancels the old one via the
    /// previous wrapper's `deinit`; clearing it after the task completes
    /// lets the wrapper drop without re-cancelling. Removing the candidate
    /// from the store drops the wrapper too, cancelling the request.
    var searchTask: CancelOnDeinit?

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

    /// One read of a selected candidate: the folder, its resumed identify
    /// state, and the row the sidebar places it as.
    init(
        detail: BridgeImportCandidateDetail
    ) {
        self.init(bridge: detail.candidate)
        resumedIdentifyState = IdentifyState(
            bridge: detail.resumedIdentifyState
        )
        row = detail.row
        self.detail = detail
        metadataPresentation = Self.initialPresentation(for: detail)
    }

    /// This row over `existing`'s session state: the list re-read the folder,
    /// and nothing about the user's work on it changed.
    func withSessionState(from existing: Candidate) -> Candidate {
        var copy = self
        copy.libraryStatuses = existing.libraryStatuses
        copy.libraryStatusSubscriptions = existing.libraryStatusSubscriptions
        copy.error = existing.error
        copy.metadataApplicationSession = existing.metadataApplicationSession
        copy.metadataPresentation = existing.metadataPresentation
        copy.fileTagsPreview =
            files.fileTagsIdentity == existing.files.fileTagsIdentity
            ? existing.fileTagsPreview : .unloaded
        copy.search = existing.search
        copy.searchTask = existing.searchTask
        return copy
    }

    /// Construct a re-identify candidate. The release already lives in the
    /// library, so this carries only the session's own work — the result, the
    /// search state. Its run's identify state comes from the candidate-runtime
    /// signal under the same key, which is how the existing `ImportSearchPane`
    /// UI renders unchanged.
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
            fileTagsIdentity:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            files: [],
            sourceAudio: nil
        )
    }

    /// The selected external release as its archived documents describe it.
    var release: BridgeReleaseDetail? {
        detail?.release
    }

    /// Identifying signals extracted from the candidate, each entry naming
    /// the source file whose gallery tile or table row carries the chip.
    var fileEvidence: [BridgeFileEvidence] {
        detail?.fileEvidence ?? []
    }

    /// The candidate's editable metadata draft.
    var edit: BridgeRawReleaseEdit? {
        detail?.metadataDraft
    }

    /// Every source unit the folder offers with the track committing makes of
    /// it. An empty table until the first read answers; the pane's own shape
    /// does not change for it.
    var mapping: BridgeMappingTable {
        detail?.mapping
            ?? BridgeMappingTable(
                images: [],
                trackSections: [],
                files: [],
                reconciliation: nil
            )
    }

    /// The cover this candidate commits with.
    var cover: BridgeCoverChoice? {
        detail?.cover
    }

    /// The last import of this candidate that failed, as it survives a
    /// relaunch.
    var failure: BridgeImportFailure? {
        detail?.failure
    }

    /// Whether the applied external release is already in the library.
    var pickedLibraryStatus: BridgeLibraryStatus? {
        detail?.pickedLibraryStatus
    }

    /// Where the current draft was populated from. Directly entered and
    /// cleared drafts have no provenance.
    var metadataProvenance: BridgeMetadataProvenance? {
        detail?.metadataProvenance
    }

    var metadataDraftIsBlank: Bool {
        detail?.metadataDraftIsBlank ?? true
    }

    var localCoverSelections: [String: BridgeCoverSelection] {
        files.images.reduce(into: [:]) { selections, image in
            if let choice = image.coverChoice {
                selections[image.file.name] = choice.selection
            }
        }
    }

    /// The external release the draft came from, where it names one.
    var pickedRelease: (source: BridgeMetadataSource, releaseId: String)? {
        guard
            case .externalRelease(let source, let releaseId) =
                metadataProvenance
        else { return nil }
        return (source: source, releaseId: releaseId)
    }

    private static func initialPresentation(
        for detail: BridgeImportCandidateDetail
    ) -> CandidateMetadataPresentation {
        guard detail.metadataProvenance == nil else { return .draft }
        return switch detail.initialMetadataSource {
        case .findOnline: .findOnline
        case .fileTags: .fileTags
        case .none: .draft
        }
    }

    /// The signals identification settled on for this candidate's files, as
    /// its stored row carries them.
    var settledSignals: Signals? {
        detail?.signals.map(Signals.init(bridge:))
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
