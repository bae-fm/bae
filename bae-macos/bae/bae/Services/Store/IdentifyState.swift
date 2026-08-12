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
    case found(
        group: ReleaseGroup,
        libraryStatuses: [String: BridgeLibraryStatus],
        trackCount: UInt32,
        /// Per-pressing provenance keyed by release id — the per-row badges, and
        /// which signal produced each match.
        provenance: [String: BridgeResultProvenance],
    )
    /// Signals disagreed: empty intersection or multi-group result. The
    /// conflict surface presents the per-signal sections so the user can
    /// pick a section, exclude a signal, or fall back to manual search.
    case conflict(
        discidResults: [BridgeMetadataResult],
        discidLibraryStatuses: [String: BridgeLibraryStatus],
        barcodeResults: [BridgeMetadataResult],
        barcodeLibraryStatuses: [String: BridgeLibraryStatus],
        matchedBarcode: String?,
        trackCount: UInt32,
    )
    case notFoundAnywhere
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `notFoundAnywhere`, where signals
    /// ran and matched nothing.
    case manualOnly(trackCount: UInt32)

    init(bridge: BridgeIdentifyState) {
        switch bridge {
        case .idle: self = .idle
        case .triangulating(let discid, let barcode):
            self = .triangulating(discid: discid, barcode: barcode)
        case .found(
            let group,
            let libraryStatuses,
            let trackCount,
            let provenance
        ):
            self = .found(
                group: ReleaseGroup(bridge: group),
                libraryStatuses: libraryStatuses,
                trackCount: trackCount,
                provenance: provenance,
            )
        case .conflict(
            let discidResults,
            let discidLibraryStatuses,
            let barcodeResults,
            let barcodeLibraryStatuses,
            let matchedBarcode,
            let trackCount
        ):
            self = .conflict(
                discidResults: discidResults,
                discidLibraryStatuses: discidLibraryStatuses,
                barcodeResults: barcodeResults,
                barcodeLibraryStatuses: barcodeLibraryStatuses,
                matchedBarcode: matchedBarcode,
                trackCount: trackCount,
            )
        case .notFoundAnywhere: self = .notFoundAnywhere
        case .manualOnly(let trackCount):
            self = .manualOnly(trackCount: trackCount)
        }
    }

}
