import BaeKit
import Foundation

/// Mirror of `bae_core::identify::IdentifyState`. The import-candidate
/// projection assigns one of these onto a candidate on every refresh; the UI
/// switches on the variant to render banners and match lists.
///
/// A settled state carries the run it settled as, so the ledger stays up
/// beside the matches. It carries none when extraction handed the run nothing
/// to lay out: a folder with no disc ID, no barcode source and no catalog
/// number, or a verdict stood back up from the store.
/// The releases the signals' agreement left out of the matches — real answers
/// a lookup returned that the agreement discarded, offered behind a disclosure
/// rather than dropped. Shaped exactly as a state's own matches, so the pane
/// lists them the same way. Empty when the agreement narrowed nothing.
struct NarrowedOut: Equatable {
    var groups: [ReleaseGroup]
    /// Library status per release, keyed by release id, as the matches' are.
    var libraryStatuses: [String: BridgeLibraryStatus]
    /// Which signals named each release, keyed by release id.
    var provenance: [String: BridgeResultProvenance]
}

extension NarrowedOut {
    /// The agreement narrowed nothing out — what every state that never
    /// intersected anything carries.
    static let nothing = NarrowedOut(
        groups: [],
        libraryStatuses: [:],
        provenance: [:]
    )

    init(bridge: BridgeNarrowedOut) {
        self.init(
            groups: bridge.groups.map(ReleaseGroup.init(bridge:)),
            libraryStatuses: bridge.libraryStatuses,
            provenance: bridge.provenance
        )
    }

    var isEmpty: Bool { groups.isEmpty }

    /// How many releases are behind the disclosure — pressings, not cards: two
    /// sources' records of one pressing are one release to pick.
    var pressingCount: Int {
        groups.reduce(0) { $0 + $1.pressings.count }
    }
}

enum IdentifyState: Equatable {
    case idle
    /// Lookups in flight, laid out as the run's ledger — one row per value
    /// extraction found, one cell per provider — with the matches the
    /// answered lookups have combined to so far, shaped as `found`'s are.
    /// The pipeline transitions to a terminal state once every step settles.
    case triangulating(
        run: BridgeIdentifyRun,
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        provenance: [String: BridgeResultProvenance],
        narrowedOut: NarrowedOut,
    )
    /// The matches as group cards, in match order. Usually one card; signals
    /// that named different releases give several, which is the same list of
    /// things to pick from either way.
    case found(
        run: BridgeIdentifyRun?,
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        trackCount: UInt32,
        /// Per-pressing provenance keyed by release id — the per-row badges, and
        /// which signal produced each match.
        provenance: [String: BridgeResultProvenance],
        narrowedOut: NarrowedOut,
    )
    case notFoundAnywhere(run: BridgeIdentifyRun?)
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `notFoundAnywhere`, where signals
    /// ran and matched nothing. The run is there when extraction found catalog
    /// numbers the person can still activate.
    case manualOnly(trackCount: UInt32, run: BridgeIdentifyRun?)
    /// A lookup failed, with whatever the surviving evidence still found: one
    /// provider failing leaves the other's matches standing. `groups` is empty
    /// when nothing answered, and for a failure resumed from its stored
    /// verdict.
    case failed(
        run: BridgeIdentifyRun?,
        failures: [BridgeIdentifyFailure],
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        provenance: [String: BridgeResultProvenance],
        narrowedOut: NarrowedOut,
    )

    init(bridge: BridgeIdentifyState) {
        switch bridge {
        case .idle: self = .idle
        case .triangulating(
            let run,
            let groups,
            let libraryStatuses,
            let provenance,
            let narrowedOut
        ):
            self = .triangulating(
                run: run,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                provenance: provenance,
                narrowedOut: NarrowedOut(bridge: narrowedOut),
            )
        case .found(
            let run,
            let groups,
            let libraryStatuses,
            let trackCount,
            let provenance,
            let narrowedOut
        ):
            self = .found(
                run: run,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                trackCount: trackCount,
                provenance: provenance,
                narrowedOut: NarrowedOut(bridge: narrowedOut),
            )
        case .notFoundAnywhere(let run): self = .notFoundAnywhere(run: run)
        case .manualOnly(let trackCount, let run):
            self = .manualOnly(trackCount: trackCount, run: run)
        case .failed(
            let run,
            let failures,
            let groups,
            let libraryStatuses,
            let provenance,
            let narrowedOut
        ):
            self = .failed(
                run: run,
                failures: failures,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                provenance: provenance,
                narrowedOut: NarrowedOut(bridge: narrowedOut),
            )
        }
    }

    /// What core knew about each matched release's library membership when the
    /// verdict settled, keyed by release id. A live subscription outranks it.
    var libraryStatuses: [String: BridgeLibraryStatus] {
        switch self {
        case .found(_, _, let statuses, _, _, _): statuses
        case .failed(_, _, _, let statuses, _, _): statuses
        case .triangulating(_, _, let statuses, _, _): statuses
        case .idle, .notFoundAnywhere, .manualOnly: [:]
        }
    }

    /// The releases the signals' agreement left out of the matches — what the
    /// pane offers behind its disclosure.
    var narrowedOut: NarrowedOut {
        switch self {
        case .found(_, _, _, _, _, let narrowedOut): narrowedOut
        case .failed(_, _, _, _, _, let narrowedOut): narrowedOut
        case .triangulating(_, _, _, _, let narrowedOut): narrowedOut
        case .idle, .notFoundAnywhere, .manualOnly: .nothing
        }
    }

    /// The run as its ledger, while there is one to lay out.
    var run: BridgeIdentifyRun? {
        switch self {
        case .triangulating(let run, _, _, _, _): run
        case .found(let run, _, _, _, _, _): run
        case .notFoundAnywhere(let run): run
        case .manualOnly(_, let run): run
        case .failed(let run, _, _, _, _, _): run
        case .idle: nil
        }
    }
}
