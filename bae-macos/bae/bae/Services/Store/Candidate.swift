import BaeKit
import Foundation

// MARK: - CandidateSource

/// Source-specific data for a candidate. Folder candidates carry the watched
/// folder they were scanned from and their own folder, each as coven's bytes
/// for its path; re-identify candidates carry the existing library release
/// id.
enum CandidateSource: Equatable {
    case folder(
        watchedFolderPath: Data,
        folderPath: Data
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
/// offers: the subscription, pulled through `ScreenSubscription`, and the
/// revision of the newest checks, so a value answering checks since replaced
/// is not shown. With no subscription, a preview's or a test's, it reads
/// nothing.
@MainActor
final class LibraryStatusObservation: Equatable {
    private let subscription: LibraryStatusSubscription?
    private var pulls: ScreenSubscription<LibraryStatusSubscription>?
    private var requested: UInt64 = 0

    init(subscription: LibraryStatusSubscription?) {
        self.subscription = subscription
    }

    /// Apply each value answering the newest checks, until the subscription
    /// ends.
    func start(
        onValue: @escaping @MainActor ([String: BridgeLibraryStatus]) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        guard let subscription else { return }
        pulls = ScreenSubscription(
            subscription,
            onValue: { [weak self] snapshot in
                guard let self, snapshot.requestRevision >= self.requested
                else { return }
                onValue(snapshot.statuses)
            },
            onError: onError
        )
    }

    func setChecks(_ checks: Set<BridgeLibraryCheck>) throws {
        guard let subscription else { return }
        requested = try subscription.setChecks(
            checks: checks.sorted {
                ($0.releaseId, "\($0.source)") < ($1.releaseId, "\($1.source)")
            }
        )
    }

    nonisolated static func == (
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

/// A pick being read: the release whose row it was made on, and how that row
/// names what the read waits on.
struct LoadingPick {
    let releaseId: String
    let reading: PickReading
}

/// Whose read a pick is, which says what its row can name.
enum PickReading {
    /// A person's pick, telling each network read it waits on as it waits;
    /// `nil` where no bridge is behind the pick.
    case followed(LatestSource<BridgeCatalogApplyStep>?)
    /// The run's own pick, which core reads in full before it stores the
    /// answer.
    case unattended
}

/// One metadata application, from its click until the read it dispatched
/// ends. The store owns it under the candidate's key; dropping that entry
/// cancels the read.
final class CandidateMetadataApplicationSession: Equatable,
    @unchecked Sendable
{
    let application: MetadataApplication
    /// The steps a pick's read reports, followed while the session lives;
    /// `nil` for a read of the files' own tags, and where no bridge is
    /// behind the pick.
    let steps: CatalogApplySteps?

    private var task: Task<Void, Never>?

    init(application: MetadataApplication, steps: CatalogApplySteps?) {
        self.application = application
        self.steps = steps
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
        case .import, .keepOwnDraft, .linkSharedAlbum, .unlink:
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
/// identifier over — the Disc ID, one of the candidate's barcodes, or one of
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
    /// The key core names the candidate by: its folder's path bytes, or
    /// its grouping's or re-identify session's key. Used as the dictionary
    /// key.
    let key: Data
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
    /// offers with it, read with its detail. `nil` for a re-identify session.
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

    var id: Data {
        key
    }

    /// The folders this release is read from, when it is several. Empty for
    /// a release read from one folder.
    var parts: [BridgeReleasePart] { detail?.candidate.parts ?? [] }
    /// Where this candidate's import stands for the pane: running now, or
    /// what the last one left.
    var importStatus: BridgeCandidateImportStatus? { detail?.importStatus }
    /// Each folder's path, as coven's bytes for it; none for a library
    /// release being identified again.
    var sourceFolderPaths: [Data] {
        switch source {
        case .folder(_, let folderPath):
            parts.isEmpty ? [folderPath] : parts.map(\.folderPath)
        case .releaseReIdentify:
            []
        }
    }

    init(bridge: BridgeFolderCandidate) {
        source = .folder(
            watchedFolderPath: bridge.watchedFolderPath,
            folderPath: bridge.folderPath
        )
        key = bridge.candidateKey
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
        if case .drafting(let draft) = detail.stage {
            session = CandidateSessionState(bridge: draft.session)
        }
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
        reIdentifyKey: Data,
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

    /// The candidate's draft and what goes with it; `nil` until the first
    /// read answers, and once this device imported its files, when it holds
    /// none.
    var draft: BridgeCandidateDraftPane? {
        guard case .drafting(let draft) = detail?.stage else { return nil }
        return draft
    }

    /// The folder's images and its other files, once this device imported
    /// its files: what the imported folder's pane shows beside the library
    /// release.
    var importedFolder: BridgeFolderFiles? {
        guard case .imported(_, let folder) = detail?.stage else { return nil }
        return folder
    }

    /// The covers the picker offers from the catalogs: what the applied
    /// release offered, stored with the draft.
    var remoteCovers: [BridgeRemoteCover] {
        draft?.remoteCovers ?? []
    }

    /// The candidate's editable metadata draft.
    var edit: BridgeRawReleaseEdit? {
        draft?.metadataDraft
    }

    /// Every source unit the folder offers with the track committing makes of
    /// it. An empty table until the first read answers; the pane's own shape
    /// does not change for it.
    var mapping: BridgeMappingTable {
        draft?.mapping
            ?? BridgeMappingTable(
                images: [],
                trackSides: [],
                files: [],
                untitledTrackCount: 0
            )
    }

    /// The cover this candidate commits with.
    var cover: BridgeCoverChoice? {
        draft?.cover
    }

    /// The last import of this candidate that failed, as it survives a
    /// relaunch.
    var failure: BridgeError? {
        draft?.failure
    }

    /// Whether the applied external release is already in the library.
    var pickedLibraryStatus: BridgeLibraryStatus? {
        draft?.pickedLibraryStatus
    }

    /// Where the current draft was populated from. Directly entered and
    /// cleared drafts have no provenance.
    var metadataProvenance: BridgeMetadataProvenance? {
        draft?.metadataProvenance
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
        draft?.metadataDraftIsBlank ?? true
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
        draft?.releaseLink
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
