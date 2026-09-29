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

/// One candidate's live read of library membership for the releases its pane
/// offers: the query, the task applying its values, and the revision of the
/// newest checks, so a value answering checks since replaced is not shown.
final class LibraryStatusObservation: Equatable, @unchecked Sendable {
    private let query: LibraryStatusQuery
    private var deliveries: Task<Void, Never>?
    private var requested: UInt64 = 0

    init(query: LibraryStatusQuery) {
        self.query = query
    }

    /// Apply each value answering the newest checks, until the query ends.
    @MainActor
    func start(
        onValue: @escaping @MainActor ([String: BridgeLibraryStatus]) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        let query = self.query
        deliveries = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                do {
                    let snapshot = try await query.next()
                    guard let self else { return }
                    if snapshot.requestRevision >= self.requested {
                        onValue(snapshot.statuses)
                    }
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch is CancellationError {
                    return
                }
                catch {
                    if !Task.isCancelled {
                        onError(error)
                    }
                    return
                }
            }
        }
    }

    @MainActor
    func setChecks(_ checks: Set<BridgeLibraryCheck>) throws {
        requested = try query.setChecks(
            checks.sorted {
                ($0.releaseId, "\($0.source)") < ($1.releaseId, "\($1.source)")
            }
        )
    }

    deinit {
        deliveries?.cancel()
        let query = self.query
        Task { await query.cancel() }
    }

    static func == (
        lhs: LibraryStatusObservation,
        rhs: LibraryStatusObservation
    ) -> Bool {
        lhs === rhs
    }
}

enum CandidateMetadataPresentation: Equatable {
    case draft
    case findOnline

    init(bridge: BridgeMetadataPresentation) {
        switch bridge {
        case .draft: self = .draft
        case .findOnline: self = .findOnline
        }
    }
}

/// What one metadata application reads the draft from: a picked release,
/// which the candidate is then linked to, or the files' own tags, which leave
/// the link as it is.
enum MetadataApplication: Equatable, Sendable {
    case pick(BridgePressingLink)
    case fileTags
}

/// One metadata application, from its click until the read it dispatched
/// ends. The store owns it under the candidate's key; dropping that entry
/// cancels the read.
final class CandidateMetadataApplicationSession: Equatable,
    @unchecked Sendable
{
    let application: MetadataApplication

    private var task: Task<Void, Never>?

    init(application: MetadataApplication) {
        self.application = application
    }

    func install(_ task: Task<Void, Never>) {
        precondition(self.task == nil)
        self.task = task
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

// MARK: - CandidateSearchState

/// The typed-search form: which query it is asking and what has been typed
/// into it. What the search turned up is not here — every configured provider
/// answers it separately, so the run lives on the candidate's runtime and the
/// pane draws it as each provider lands.
struct CandidateSearchState: Equatable {
    var searchArtist: String = ""
    var searchAlbum: String = ""
    var searchCatalog: String = ""
    var searchBarcode: String = ""
    var activeTab: SearchTab = .general

    init(
        searchArtist: String = "",
        searchAlbum: String = "",
        searchCatalog: String = "",
        searchBarcode: String = "",
        activeTab: SearchTab = .general
    ) {
        self.searchArtist = searchArtist
        self.searchAlbum = searchAlbum
        self.searchCatalog = searchCatalog
        self.searchBarcode = searchBarcode
        self.activeTab = activeTab
    }

    init(bridge: BridgeSearchForm) {
        searchArtist = bridge.artist
        searchAlbum = bridge.album
        searchCatalog = bridge.catalog
        searchBarcode = bridge.barcode
        activeTab =
            switch bridge.tab {
            case .general: .general
            case .catalogNumber: .catalogNumber
            case .barcode: .barcode
            }
    }

    var bridge: BridgeSearchForm {
        let tab: BridgeSearchTab =
            switch activeTab {
            case .general: .general
            case .catalogNumber: .catalogNumber
            case .barcode: .barcode
            }
        return BridgeSearchForm(
            tab: tab,
            artist: searchArtist,
            album: searchAlbum,
            catalog: searchCatalog,
            barcode: searchBarcode
        )
    }
}

// MARK: - CandidateSessionState

/// Where the pane was when the person last left this candidate: which
/// surface the metadata slot shows, the typed-search form, and the last
/// command that failed. Core stores it with the candidate, so clicking away
/// and a relaunch both come back to the same pane; a re-identify session,
/// which has no stored candidate, keeps its own in memory.
struct CandidateSessionState: Equatable {
    var presentation: CandidateMetadataPresentation = .draft
    /// The section of the Find online page open when it shows.
    var findOnlineSection: BridgeFindOnlineSection = .automatic
    var search = CandidateSearchState()
    /// The last command this pane ran, when it failed, as core stored it.
    /// Shown in the banner until the pane's next command clears it.
    var failure: BridgePaneFailure?

    init() {}

    init(bridge: BridgeCandidateSession) {
        presentation = CandidateMetadataPresentation(
            bridge: bridge.presentation
        )
        findOnlineSection = bridge.findOnlineSection
        search = CandidateSearchState(bridge: bridge.search)
        failure = bridge.error
    }
}

extension BridgePaneFailure {
    /// The banner's line: what the command was doing, where saying so helps,
    /// and why it failed.
    var line: String? {
        guard let displayed = DisplayError(error) else { return nil }
        let why = displayed.line
        switch command {
        case .import, .mergeArtists, .keepOwnDraft, .linkSharedAlbum, .unlink:
            return why
        case .readFileTags:
            return String(localized: "Couldn't read file tags: \(why)")
        case .changeLookups:
            return String(
                localized:
                    "Couldn't change what identification looks up: \(why)"
            )
        case .changeSearchWords:
            return String(
                localized:
                    "Couldn't change what identification searches by: \(why)"
            )
        case .changeAgreements:
            return String(
                localized: "Couldn't change what counts as an agreement: \(why)"
            )
        }
    }
}

// MARK: - Lookup toggles

/// Which identifier a chip in the band stands for. Every chip turns its own
/// identifier over — the disc ID, one of the candidate's barcodes, or one of
/// its catalog numbers — and that change is what goes to core.
enum LookupToggle: Equatable {
    case discId
    case barcode(String)
    case catalog(String)

    /// The change turning this identifier over makes to the choices core
    /// holds.
    var edit: BridgeLookupChoiceEdit {
        switch self {
        case .discId: .toggleDiscId
        case .barcode(let code): .toggleBarcode(code: code)
        case .catalog(let number): .toggleCatalog(number: number)
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
    /// Where the queue places this candidate, with what the pane states
    /// beside it — read by key alongside the folder. `nil` for a re-identify
    /// session, which has no scanned folder and so no place in the queue.
    var placement: BridgeCandidatePanePlacement?
    /// What is running for this candidate right now and the commands it
    /// offers with it, read with its row. `nil` for a re-identify session.
    var live: BridgeCandidateLiveState?
    var libraryStatuses: [String: BridgeLibraryStatus] = [:]
    var libraryStatusObservation: LibraryStatusObservation?
    /// Where the pane was when the person last left this candidate. A folder
    /// candidate's comes with its detail; a re-identify session's lives here.
    var session = CandidateSessionState()
    /// The draft, or the Find online page, occupying the metadata slot.
    /// Opening the page never replaces the stored draft; applying a result
    /// does.
    var metadataPresentation: CandidateMetadataPresentation {
        session.presentation
    }
    var search: CandidateSearchState { session.search }
    /// The last command this pane ran, when it failed.
    var error: String? { session.failure?.line }

    var id: String {
        key
    }

    /// The folders this release is read from, when it is several. Empty for
    /// a release read from one folder.
    var parts: [BridgeReleasePart] { detail?.candidate.parts ?? [] }
    /// Where this candidate's import stands for the pane: running now, or
    /// what the last one left.
    var importStatus: BridgeCandidateImportStatus? { detail?.importStatus }
    var sourceFolderPaths: [String] {
        if !parts.isEmpty { return parts.map(\.folderPath) }
        return [key]
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
    /// state, and where the queue places it.
    init(
        detail: BridgeImportCandidateDetail
    ) {
        self.init(bridge: detail.candidate)
        resumedIdentifyState = IdentifyState(
            bridge: detail.resumedIdentifyState
        )
        placement = detail.placement
        live = detail.live
        self.detail = detail
        session = CandidateSessionState(bridge: detail.session)
    }

    /// This row over `existing`'s session state: the list re-read the folder,
    /// and nothing about the user's work on it changed. A pick in flight is
    /// not here — the store holds that under the candidate's key, where no
    /// selection change reaches it.
    func withSessionState(from existing: Candidate) -> Candidate {
        var copy = self
        copy.libraryStatuses = existing.libraryStatuses
        copy.libraryStatusObservation = existing.libraryStatusObservation
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
            fileMetadataIdentity:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            files: [],
            coverFiles: [],
            sourceAudio: nil
        )
    }

    /// The selected external release as its stored release rows describe it.
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
                files: []
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

    /// Every catalog that describes the release this candidate's draft was
    /// read from, in the order core lists them. Empty for a draft read from
    /// the files' own tags, typed in, or not there yet.
    var records: [BridgeReleaseRecord] {
        switch placement {
        case .pending(_, let records), .skipped(let records): records
        case .done, nil: []
        }
    }

    /// The tab the queue places this candidate on.
    var tab: BridgeTriageTab? {
        switch placement {
        case .pending: .pending
        case .skipped: .skipped
        case .done: .done
        case nil: nil
        }
    }

    /// The check against the folder this candidate's found release did not
    /// pass, stated beside its Import.
    var folderCheck: BridgeFolderCheck? {
        guard case .pending(let folderCheck, _) = placement else { return nil }
        return folderCheck
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

    /// What the candidate is linked to in the catalogs: the pressing picked,
    /// or the album whose pressing is unknown, whatever its draft was read
    /// from since.
    var releaseLink: BridgeReleaseLink? {
        detail?.releaseLink
    }

    /// The signals identification settled on for this candidate's files, as
    /// its stored row carries them.
    var settledSignals: Signals? {
        detail?.signals.map(Signals.init(bridge:))
    }

}

extension BridgeReleaseLink {
    /// The pressing the link names; `nil` for an album, whose pressing is
    /// unknown.
    var pressing: BridgePressingLink? {
        switch self {
        case .pressing(let link): link
        case .album: nil
        }
    }
}
