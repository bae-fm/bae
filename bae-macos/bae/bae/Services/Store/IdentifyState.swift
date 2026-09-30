import BaeKit
import Foundation

/// Mirror of `bae_core::identify::IdentifyState`, with the wait on the
/// identification queue before a run starts, which core reports beside it.
/// The import-candidate projection assigns one of these onto a candidate on
/// every refresh; the UI switches on the variant to render banners and match
/// lists.
///
/// A settled state carries the run it settled as, so the ledger stays up
/// beside the matches. It carries none when extraction handed the run nothing
/// to lay out: a folder with no disc ID, no barcode source and no catalog
/// number, or a verdict stood back up from the store.
///
/// A state's `groups` are every card its answers make. The rows agreement left
/// out — real answers a lookup returned that the intersection discarded, and
/// the ones the folder's own text says nothing about — sit on their album's
/// card as its sections' `narrowedOut`, offered behind a disclosure rather
/// than dropped; a card all of whose rows were left out comes after every
/// card that offers one. `narrowedOutCount` is how many rows are behind the
/// disclosure, on every card — pressings, not cards: two sources' records of
/// one pressing are one release to pick.
enum IdentifyState: Equatable {
    case idle
    /// On the identification queue, its run not started. Only a run in
    /// flight says so; a stored verdict never does.
    case queued
    /// Lookups in flight, laid out as the run's ledger — one row per value
    /// extraction found, one cell per provider — with the matches the
    /// answered lookups have combined to so far, shaped as `found`'s are.
    /// The pipeline transitions to a terminal state once every step settles.
    case triangulating(
        run: BridgeIdentifyRun,
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        agreements: [String: BridgeAgreements],
        narrowedOutCount: UInt32,
    )
    /// The matches as group cards, ranked — most agreed with first, rendered
    /// in the order they arrive. Usually one card; signals that named
    /// different releases give several, which is the same list of things to
    /// pick from either way.
    case found(
        run: BridgeIdentifyRun?,
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        trackCount: UInt32,
        /// What the candidate's own text agrees with about each pressing,
        /// keyed by release id — the per-row badges, and what ordered the rows.
        agreements: [String: BridgeAgreements],
        narrowedOutCount: UInt32,
        /// The catalog numbers the folder states about the offered releases —
        /// the Catalog # row's chips. Striking one out re-ranks the list with
        /// nothing asked again.
        catalogAgreements: [BridgeCatalogAgreement],
        /// The check against the folder the found release failed: why
        /// nothing was picked.
        folderCheck: BridgeFolderCheck?,
        /// Whether core picks the one release for the folder on its own.
        picksUnattended: Bool,
        /// Whether the offered rows are several pressings of one album, which
        /// the folder can be linked to with its pressing unknown.
        offersSharedAlbum: Bool,
    )
    case notFoundAnywhere(run: BridgeIdentifyRun?)
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `notFoundAnywhere`, where signals
    /// ran and matched nothing. The run is there when extraction found catalog
    /// numbers the person can still activate.
    case manualOnly(trackCount: UInt32, run: BridgeIdentifyRun?)
    /// bae broke on its own side and the run ended there, with why. Nothing
    /// it found stands.
    case error(failure: BridgeInternalFailure)
    /// A lookup failed, with whatever the surviving evidence still found: one
    /// provider failing leaves the other's matches standing, live or resumed
    /// from the stored verdict. `groups` is empty when nothing that answered
    /// returned anything.
    case failed(
        run: BridgeIdentifyRun?,
        failures: [BridgeIdentifyFailure],
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        agreements: [String: BridgeAgreements],
        narrowedOutCount: UInt32,
        catalogAgreements: [BridgeCatalogAgreement],
        offersSharedAlbum: Bool,
    )

    // One case per bridge variant, copied field for field.
    // swiftlint:disable:next function_body_length
    init(bridge: BridgeIdentifyState) {
        switch bridge {
        case .idle: self = .idle
        case .triangulating(
            let run,
            let groups,
            let libraryStatuses,
            let agreements,
            let narrowedOutCount
        ):
            self = .triangulating(
                run: run,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                agreements: agreements,
                narrowedOutCount: narrowedOutCount,
            )
        case .found(
            let run,
            let groups,
            let libraryStatuses,
            let trackCount,
            let agreements,
            let narrowedOutCount,
            let catalogAgreements,
            let folderCheck,
            let picksUnattended,
            let offersSharedAlbum
        ):
            self = .found(
                run: run,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                trackCount: trackCount,
                agreements: agreements,
                narrowedOutCount: narrowedOutCount,
                catalogAgreements: catalogAgreements,
                folderCheck: folderCheck,
                picksUnattended: picksUnattended,
                offersSharedAlbum: offersSharedAlbum,
            )
        case .notFoundAnywhere(let run): self = .notFoundAnywhere(run: run)
        case .manualOnly(let trackCount, let run):
            self = .manualOnly(trackCount: trackCount, run: run)
        case .error(let failure): self = .error(failure: failure)
        case .failed(
            let run,
            let failures,
            let groups,
            let libraryStatuses,
            let agreements,
            let narrowedOutCount,
            let catalogAgreements,
            let offersSharedAlbum
        ):
            self = .failed(
                run: run,
                failures: failures,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                agreements: agreements,
                narrowedOutCount: narrowedOutCount,
                catalogAgreements: catalogAgreements,
                offersSharedAlbum: offersSharedAlbum,
            )
        }
    }

    /// What core knew about each matched release's library membership when the
    /// verdict settled, keyed by release id. A live subscription outranks it.
    var libraryStatuses: [String: BridgeLibraryStatus] {
        switch self {
        case .found(_, _, let statuses, _, _, _, _, _, _, _): statuses
        case .failed(_, _, _, let statuses, _, _, _, _): statuses
        case .triangulating(_, _, let statuses, _, _): statuses
        case .idle, .queued, .notFoundAnywhere, .manualOnly, .error: [:]
        }
    }

    /// How many rows the signals' agreement left out of the matches — what
    /// the pane's disclosure counts.
    var narrowedOutCount: UInt32 {
        switch self {
        case .found(_, _, _, _, _, let count, _, _, _, _): count
        case .failed(_, _, _, _, _, let count, _, _): count
        case .triangulating(_, _, _, _, let count): count
        case .idle, .queued, .notFoundAnywhere, .manualOnly, .error: 0
        }
    }

    /// The catalog numbers the folder states about the releases identification
    /// is offering — the Catalog # row's chips. A run still going has none:
    /// which numbers these are follows from the releases it settles on.
    var catalogAgreements: [BridgeCatalogAgreement] {
        switch self {
        case .found(_, _, _, _, _, _, let chips, _, _, _): chips
        case .failed(_, _, _, _, _, _, let chips, _): chips
        case .idle, .queued, .triangulating, .notFoundAnywhere, .manualOnly,
            .error:
            []
        }
    }

    /// The check against the folder the found release failed, which says
    /// why nothing was picked.
    var folderCheck: BridgeFolderCheck? {
        switch self {
        case .found(_, _, _, _, _, _, _, let folderCheck, _, _): folderCheck
        case .idle, .queued, .triangulating, .notFoundAnywhere, .manualOnly,
            .failed, .error:
            nil
        }
    }

    /// Whether the offered rows are several pressings of one album, which
    /// the folder can be linked to with its pressing unknown.
    var offersSharedAlbum: Bool {
        switch self {
        case .found(_, _, _, _, _, _, _, _, _, let offers): offers
        case .failed(_, _, _, _, _, _, _, let offers): offers
        case .idle, .queued, .triangulating, .notFoundAnywhere, .manualOnly,
            .error:
            false
        }
    }

    /// Whether core picks the found release for the folder on its own.
    var picksUnattended: Bool {
        switch self {
        case .found(_, _, _, _, _, _, _, _, let picks, _): picks
        case .idle, .queued, .triangulating, .notFoundAnywhere, .manualOnly,
            .failed, .error:
            false
        }
    }

    /// The run as its ledger, while there is one to lay out.
    var run: BridgeIdentifyRun? {
        switch self {
        case .triangulating(let run, _, _, _, _): run
        case .found(let run, _, _, _, _, _, _, _, _, _): run
        case .notFoundAnywhere(let run): run
        case .manualOnly(_, let run): run
        case .failed(let run, _, _, _, _, _, _, _): run
        case .idle, .queued, .error: nil
        }
    }
}
