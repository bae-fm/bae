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

struct ReleaseLibraryStatusSubscriptionKey: Hashable {
    let source: BridgeCatalog
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

    init(bridge: BridgeMetadataPresentation) {
        switch bridge {
        case .draft: self = .draft
        case .findOnline: self = .findOnline
        }
    }

    var bridge: BridgeMetadataPresentation {
        switch self {
        case .draft: .draft
        case .findOnline: .findOnline
        }
    }
}

/// One metadata application, from its click until the read it dispatched
/// ends. The store owns it under the candidate's key; dropping that entry
/// cancels the read.
final class CandidateMetadataApplicationSession: Equatable,
    @unchecked Sendable
{
    let provenance: BridgeMetadataProvenance

    private var task: Task<Void, Never>?

    init(provenance: BridgeMetadataProvenance) {
        self.provenance = provenance
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
    var search = CandidateSearchState()
    /// The last command this pane ran, when it failed — a selection whose
    /// read dropped, a write that would not land, a commit the fields do not
    /// support. Shown in the banner and cleared by the next command.
    var error: String?

    init() {}

    init(bridge: BridgeCandidateSession) {
        presentation = CandidateMetadataPresentation(
            bridge: bridge.presentation
        )
        search = CandidateSearchState(bridge: bridge.search)
        error = bridge.error
    }
}

// MARK: - BridgeLookupChoices

/// Which identifier a chip in the band stands for. Every chip turns its own
/// identifier over — the disc ID, one of the candidate's barcodes, or one of
/// its catalog numbers — and the whole value of what identification asks about
/// is what goes back.
enum LookupToggle: Equatable {
    case discId
    case barcode(String)
    case catalog(String)
}

extension BridgeLookupChoices {
    /// This value with one identifier turned over: a disc ID asked about or
    /// left out, a barcode left out or asked about again, a catalog number
    /// looked up or dropped from the run. The whole value is what a control
    /// sends back, so the change it makes is made here.
    func toggling(_ toggle: LookupToggle) -> BridgeLookupChoices {
        switch toggle {
        case .discId:
            return BridgeLookupChoices(
                discIdExcluded: !discIdExcluded,
                excludedBarcodes: excludedBarcodes,
                chosenCatalogs: chosenCatalogs,
                discountedCatalogs: discountedCatalogs
            )
        case .barcode(let code):
            // A set, so it goes back sorted and each code appears once.
            var leftOut = Set(excludedBarcodes)
            if leftOut.remove(code) == nil {
                leftOut.insert(code)
            }
            return BridgeLookupChoices(
                discIdExcluded: discIdExcluded,
                excludedBarcodes: leftOut.sorted(),
                chosenCatalogs: chosenCatalogs,
                discountedCatalogs: discountedCatalogs
            )
        case .catalog(let number):
            return choosing(number)
        }
    }

    /// This value with `catalog` among the numbers the run looks up, or
    /// without it when it already was. The chosen numbers are dispatched in the
    /// order they were chosen, so they are a list rather than a set.
    func choosing(_ catalog: String) -> BridgeLookupChoices {
        var chosen = chosenCatalogs
        if let index = chosen.firstIndex(of: catalog) {
            chosen.remove(at: index)
        }
        else {
            chosen.append(catalog)
        }
        return BridgeLookupChoices(
            discIdExcluded: discIdExcluded,
            excludedBarcodes: excludedBarcodes,
            chosenCatalogs: chosen,
            discountedCatalogs: discountedCatalogs
        )
    }

    /// This value with `catalog` struck out of what the folder is taken to
    /// state, or counted again when it already was struck out. A set, so it
    /// goes back sorted and each value appears once.
    ///
    /// A struck-out number is never a chosen one, so striking it out takes it
    /// out of the numbers the run looks up. Counting it again puts it back
    /// only when it is the picked record's own number — `pickedNumber` —
    /// since keeping that agreement is what chose it in the first place.
    func discounting(
        _ catalog: String,
        pickedNumber: String?
    ) -> BridgeLookupChoices {
        var discounted = Set(discountedCatalogs)
        var chosen = chosenCatalogs
        if discounted.remove(catalog) == nil {
            discounted.insert(catalog)
            chosen.removeAll { $0 == catalog }
        }
        else if pickedNumber == catalog, !chosen.contains(catalog) {
            chosen.append(catalog)
        }
        return BridgeLookupChoices(
            discIdExcluded: discIdExcluded,
            excludedBarcodes: excludedBarcodes,
            chosenCatalogs: chosen,
            discountedCatalogs: discounted.sorted()
        )
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
    /// Where the pane was when the person last left this candidate. A folder
    /// candidate's comes with its detail; a re-identify session's lives here.
    var session = CandidateSessionState()
    /// What this candidate's identification asks about — the signals its runs
    /// leave out and the catalog numbers they look up — and the numbers struck
    /// out of what its own text is taken to state. A folder candidate's is
    /// stored with it and comes back on its detail; a re-identify session has
    /// no candidate row to store one on, so its own lives here for as long as
    /// the sheet does.
    var lookupChoices = BridgeLookupChoices(
        discIdExcluded: false,
        excludedBarcodes: [],
        chosenCatalogs: [],
        discountedCatalogs: []
    )
    /// The draft, or the Find online page, occupying the metadata slot.
    /// Opening the page never replaces the stored draft; applying a result
    /// does.
    var metadataPresentation: CandidateMetadataPresentation {
        session.presentation
    }
    var search: CandidateSearchState { session.search }
    /// The last command this pane ran, when it failed.
    var error: String? { session.error }

    var id: String {
        key
    }

    var combination: BridgeCombination? { detail?.candidate.combination }
    var sourceFileEditsAllowed: Bool {
        detail?.candidate.sourceFileEditsAllowed ?? false
    }
    var sourceFolderPaths: [String] {
        if let combination { return combination.parts.map(\.candidateKey) }
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
        session = CandidateSessionState(bridge: detail.session)
        lookupChoices = detail.lookupChoices
    }

    /// This row over `existing`'s session state: the list re-read the folder,
    /// and nothing about the user's work on it changed. A pick in flight is
    /// not here — the store holds that under the candidate's key, where no
    /// selection change reaches it.
    func withSessionState(from existing: Candidate) -> Candidate {
        var copy = self
        copy.libraryStatuses = existing.libraryStatuses
        copy.libraryStatusSubscriptions = existing.libraryStatusSubscriptions
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

    /// Every catalog that describes the release this candidate's draft was
    /// read from, in the order core lists them. Empty for a draft read from
    /// the files' own tags, typed in, or not there yet.
    var records: [BridgeReleaseRecord] {
        switch row?.reading {
        case .identified(let records): records
        case .unidentified, .prefilled, nil: []
        }
    }

    /// Every name this candidate's folder states, one line per value and in
    /// the order core lists mark kinds. Empty until something has read it.
    var marks: [BridgeReleaseMark] {
        row?.marks ?? []
    }

    /// What the rip databases said about this candidate's audio. `nil` until
    /// something has read its log, and for a folder whose log states nothing
    /// about its bits.
    var verification: BridgeVerification? {
        row?.verification
    }

    /// Who wrote the current draft. `.nobody` for a candidate with no pick,
    /// and for a re-identify session, which has no candidate row to write one
    /// on.
    var metadataAuthor: BridgeMetadataAuthor {
        detail?.metadataAuthor ?? .nobody
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

    /// The catalog's release the draft was read from, where it names one. The
    /// partners the same pick carried are the provenance's to say.
    var pickedRelease: BridgeMetadataRef? {
        guard case .externalRelease(let record, _) = metadataProvenance
        else { return nil }
        return record
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
