import BaeKit
import Foundation

/// Mirror of `bae_core::identify::IdentifyState`. The import-candidate
/// projection assigns one of these onto a candidate on every refresh; the UI
/// switches on the variant to render banners and match lists.
enum IdentifyState: Equatable {
    case idle
    /// Both signals running in parallel. Per-signal progress lets the UI
    /// show side-by-side pipes ("Computing disc-id ✓ · Looking up barcode
    /// 2 of 3..."). The pipeline transitions to a terminal state once
    /// both pipes settle.
    case triangulating(
        discid: BridgeDiscidProgress,
        barcode: BridgeBarcodeProgress
    )
    /// The matches as group cards, in match order. Usually one card; signals
    /// that named different releases give several, which is the same list of
    /// things to pick from either way.
    case found(
        groups: [ReleaseGroup],
        libraryStatuses: [String: BridgeLibraryStatus],
        trackCount: UInt32,
        /// Per-pressing provenance keyed by release id — the per-row badges, and
        /// which signal produced each match.
        provenance: [String: BridgeResultProvenance],
    )
    case notFoundAnywhere
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `notFoundAnywhere`, where signals
    /// ran and matched nothing.
    case manualOnly(trackCount: UInt32)
    case failed([BridgeIdentifyFailure])

    init(bridge: BridgeIdentifyState) {
        switch bridge {
        case .idle: self = .idle
        case .triangulating(let discid, let barcode):
            self = .triangulating(discid: discid, barcode: barcode)
        case .found(
            let groups,
            let libraryStatuses,
            let trackCount,
            let provenance
        ):
            self = .found(
                groups: groups.map(ReleaseGroup.init(bridge:)),
                libraryStatuses: libraryStatuses,
                trackCount: trackCount,
                provenance: provenance,
            )
        case .notFoundAnywhere: self = .notFoundAnywhere
        case .manualOnly(let trackCount):
            self = .manualOnly(trackCount: trackCount)
        case .failed(let failures):
            self = .failed(failures)
        }
    }

}
