import BaeKit
import Foundation

// MARK: - CandidateSource

/// Source-specific data for a candidate. Folder candidates carry the watched
/// folder the scan found them in and the folders their files come from, each
/// as coven's bytes for its path; re-identify candidates carry the existing
/// library release id.
enum CandidateSource: Equatable {
    case folder(
        watchedFolderPath: Data,
        folders: CandidateFolders
    )
    case releaseReIdentify(releaseId: String)
}

/// The folders a folder candidate's files come from, as its read reports
/// them: one folder, or the parts of a release that combines several.
enum CandidateFolders: Equatable {
    case one(folderPath: Data)
    case several(parts: [BridgeReleasePart])

    /// The candidate's read counts its parts in SQL, and a count of 0 means
    /// one folder.
    init(detail: BridgeImportCandidateDetail) {
        self =
            detail.partCount == 0
            ? .one(folderPath: detail.candidate.folderPath)
            : .several(parts: detail.candidate.parts)
    }
}

// MARK: - SearchTab

enum SearchTab: Hashable {
    case general
    case catalogNumber
    case barcode
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

/// Which section of the Find online page is open.
enum FindOnlineSection: Equatable {
    /// Identification: the run's ledger and what it matched.
    case automatic
    /// The typed search: its form and what it turned up.
    case search
}

/// Every way the person, or a command they ran, moves the pane between the
/// draft and the Find online page.
enum PaneMove: Equatable {
    /// Identification's results.
    case automatic
    /// The typed search.
    case search
    /// One section of the Find online page.
    case openSection(FindOnlineSection)
    /// The Find online page, on the section last open.
    case findOnline
    /// A source the person picked landed in the draft.
    case picked
    /// Back to the draft.
    case back
}

/// Where the pane stands: the draft, or the Find online page. The section
/// of the Find online page stays while the draft shows, so the page opens on
/// it again.
struct CandidatePanePosition: Equatable {
    var presentation: CandidateMetadataPresentation
    var findOnlineSection: FindOnlineSection
}

/// What one metadata application reads the draft from: a picked release,
/// which the candidate is then linked to, or the files' own tags, which leave
/// the link as it is.
enum MetadataApplication: Equatable, Sendable {
    case pick(BridgePressingLink)
    case fileTags
}

/// The pick a person made on a row while core reads it: the release the row
/// shows first, which identifies that row, and the apply that holds the step
/// core's read is at.
struct LoadingPick {
    let pressing: BridgeLookupRef
    /// `nil` when no bridge runs the read, as in a shot or a test.
    let apply: (any CatalogApplySource)?
}

/// Whose read a pick is, which says what its row can name.
enum PickReading {
    /// A person's pick, whose apply's row holds each network read it waits
    /// on while it waits; `nil` where no bridge is behind the pick.
    case followed((any CatalogApplySource)?)
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
    /// The apply a pick's read writes its steps to, whose step the row
    /// reads while the session lives; `nil` for a read of the files' own
    /// tags, and where no bridge is behind the pick.
    let apply: (any CatalogApplySource)?

    private var task: Task<Void, Never>?

    init(application: MetadataApplication, apply: (any CatalogApplySource)?) {
        self.application = application
        self.apply = apply
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
}

// MARK: - CandidatePaneState

/// What the pane holds of a candidate while it shows it: where the person
/// moved it and what they typed in the search form. Only the screen has this
/// state, so the pane holds it and no table stores it; showing another
/// candidate forgets it.
struct CandidatePaneState: Equatable {
    /// The surface the person moved the pane to; `nil` until they move it,
    /// while the pane stands where core says it opens.
    var presentation: CandidateMetadataPresentation?
    /// The section of the Find online page the person last opened, which
    /// stays while the draft shows. Find online opens on Automatic until
    /// they open another.
    var findOnlineSection: FindOnlineSection = .automatic
    var search = CandidateSearchState()

    /// Write what `paneMove` says: the surface it moves the pane to, and the
    /// section of the Find online page when it names one. A move that names
    /// no section leaves the one last opened.
    mutating func apply(_ paneMove: PaneMove) {
        switch paneMove {
        case .automatic:
            presentation = .findOnline
            findOnlineSection = .automatic
        case .search:
            presentation = .findOnline
            findOnlineSection = .search
        case .openSection(let section):
            presentation = .findOnline
            findOnlineSection = section
        case .findOnline:
            presentation = .findOnline
        case .picked, .back:
            presentation = .draft
        }
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
            return String(localized: "Could not read file tags: \(why)")
        case .changeLookups:
            return String(
                localized:
                    "Could not change what identification looks up: \(why)"
            )
        case .changeSearchWords:
            return String(
                localized:
                    "Could not change what identification searches by: \(why)"
            )
        case .changeAgreements:
            return String(
                localized:
                    "Could not change what counts as an agreement: \(why)"
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
    /// Everything the pane draws, as core reads it back for this key: the
    /// metadata draft and provenance, mapping table, cover, and the
    /// last failed import. `nil` for a re-identify session, which has no
    /// folder. The app makes each folder candidate from its read, and only a
    /// fixture makes one without it.
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
    /// What the pane holds of this candidate while it shows it.
    var pane = CandidatePaneState()
    /// Where the pane stands: the surface the person moved it to, or else
    /// the one core says it opens on, with the section of the Find online
    /// page the person last opened.
    var panePosition: CandidatePanePosition {
        CandidatePanePosition(
            presentation: pane.presentation
                ?? draft.map {
                    CandidateMetadataPresentation(bridge: $0.opensOn)
                } ?? .draft,
            findOnlineSection: pane.findOnlineSection
        )
    }
    /// The draft, or the Find online page, occupying the metadata slot.
    /// Opening the page never replaces the stored draft; applying a result
    /// does.
    var metadataPresentation: CandidateMetadataPresentation {
        panePosition.presentation
    }
    var search: CandidateSearchState { pane.search }
    /// The last command this pane ran, when it failed, as core stored it.
    /// Shown in the banner until the pane's next command clears it.
    var error: String? { draft?.paneFailure?.line }

    var id: Data {
        key
    }

    /// Where this candidate's import stands for the pane: running now, or
    /// what the last one left.
    var importStatus: BridgeCandidateImportStatus? { detail?.importStatus }
    /// Each folder's path, as coven's bytes for it; none for a library
    /// release being identified again.
    var sourceFolderPaths: [Data] {
        switch source {
        case .folder(_, .one(let folderPath)):
            [folderPath]
        case .folder(_, .several(let parts)):
            parts.map(\.folderPath)
        case .releaseReIdentify:
            []
        }
    }

    /// A folder candidate, and the folders its files come from.
    init(folder: BridgeFolderCandidate, folders: CandidateFolders) {
        source = .folder(
            watchedFolderPath: folder.watchedFolderPath,
            folders: folders
        )
        key = folder.candidateKey
        displayName = folder.sourceFolderName
        files = folder.files
    }

    /// One read of a selected candidate: the folder, and where the queue
    /// places it.
    init(
        detail: BridgeImportCandidateDetail
    ) {
        self.init(
            folder: detail.candidate,
            folders: CandidateFolders(detail: detail)
        )
        placement = detail.placement
        live = detail.live
        self.detail = detail
    }

    /// Construct a re-identify candidate. The release already lives in the
    /// library, so this carries only the session's own work — the result, the
    /// search state. Its run's identify state comes from the candidate-runtime
    /// signal under the same key, which is how the existing `MacFindOnline`
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

    /// The folder's images, once this device imported its files: the
    /// gallery the imported folder's pane shows beside the library release;
    /// `nil` before then and when the folder holds no image.
    var importedImages: [BridgeMappingImage]? {
        guard case .imported(_, let images) = detail?.stage else { return nil }
        return images
    }

    /// The candidate's editable metadata draft.
    var edit: BridgeRawReleaseEdit? {
        draft?.metadataDraft
    }

    /// Every source unit the folder offers with the track committing makes of
    /// it; `nil` until the first read answers, and once this device imported
    /// its files.
    var mapping: BridgeMappingTable? {
        draft?.mapping
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

    /// Every catalog that describes the release the candidate is linked
    /// to, in the order core lists them; `nil` while it is linked to none.
    var records: [BridgeReleaseRecord]? {
        switch placement {
        case .pending(_, let records), .skipped(let records): records
        case .done, nil: nil
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
