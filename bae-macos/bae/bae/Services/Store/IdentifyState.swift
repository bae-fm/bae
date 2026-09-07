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
    )

    init(bridge: BridgeIdentifyState) {
        switch bridge {
        case .idle: self = .idle
        case .triangulating(
            let run,
            let groups,
            let libraryStatuses,
            let provenance
        ):
            self = .triangulating(
                run: run,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                provenance: provenance,
            )
        case .found(
            let run,
            let groups,
            let libraryStatuses,
            let trackCount,
            let provenance
        ):
            self = .found(
                run: run,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                trackCount: trackCount,
                provenance: provenance,
            )
        case .notFoundAnywhere(let run): self = .notFoundAnywhere(run: run)
        case .manualOnly(let trackCount, let run):
            self = .manualOnly(trackCount: trackCount, run: run)
        case .failed(
            let run,
            let failures,
            let groups,
            let libraryStatuses,
            let provenance
        ):
            self = .failed(
                run: run,
                failures: failures,
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                provenance: provenance,
            )
        }
    }

    /// What core knew about each matched release's library membership when the
    /// verdict settled, keyed by release id. A live subscription outranks it.
    var libraryStatuses: [String: BridgeLibraryStatus] {
        switch self {
        case .found(_, _, let statuses, _, _): statuses
        case .failed(_, _, _, let statuses, _): statuses
        case .triangulating(_, _, let statuses, _): statuses
        case .idle, .notFoundAnywhere, .manualOnly: [:]
        }
    }

    /// The run as its ledger, while there is one to lay out.
    var run: BridgeIdentifyRun? {
        switch self {
        case .triangulating(let run, _, _, _): run
        case .found(let run, _, _, _, _): run
        case .notFoundAnywhere(let run): run
        case .manualOnly(_, let run): run
        case .failed(let run, _, _, _, _): run
        case .idle: nil
        }
    }
}
