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
        source: BridgeIdentifySource,
        /// Per-pressing provenance keyed by release id — the per-row badges.
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
        discidSourceLabel: String?,
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
            let source,
            let provenance
        ):
            self = .found(
                group: ReleaseGroup(bridge: group),
                libraryStatuses: libraryStatuses,
                trackCount: trackCount,
                source: source,
                provenance: provenance,
            )
        case .conflict(
            let discidResults,
            let discidLibraryStatuses,
            let barcodeResults,
            let barcodeLibraryStatuses,
            let discidSourceLabel,
            let matchedBarcode,
            let trackCount
        ):
            self = .conflict(
                discidResults: discidResults,
                discidLibraryStatuses: discidLibraryStatuses,
                barcodeResults: barcodeResults,
                barcodeLibraryStatuses: barcodeLibraryStatuses,
                discidSourceLabel: discidSourceLabel,
                matchedBarcode: matchedBarcode,
                trackCount: trackCount,
            )
        case .notFoundAnywhere: self = .notFoundAnywhere
        case .manualOnly(let trackCount):
            self = .manualOnly(trackCount: trackCount)
        }
    }

    /// Drop embedded library statuses matching `shouldRemove`. Terminal
    /// states carry the statuses computed when identify ran; after a library
    /// removal those entries are stale and an absent entry renders as "not
    /// in your library".
    mutating func removeLibraryStatuses(
        where shouldRemove: (String, BridgeLibraryStatus) -> Bool
    ) {
        func kept(
            _ statuses: [String: BridgeLibraryStatus]
        ) -> [String: BridgeLibraryStatus] {
            statuses.filter { !shouldRemove($0.key, $0.value) }
        }
        switch self {
        case .found(
            let group,
            let libraryStatuses,
            let trackCount,
            let source,
            let provenance
        ):
            self = .found(
                group: group,
                libraryStatuses: kept(libraryStatuses),
                trackCount: trackCount,
                source: source,
                provenance: provenance,
            )
        case .conflict(
            let discidResults,
            let discidLibraryStatuses,
            let barcodeResults,
            let barcodeLibraryStatuses,
            let discidSourceLabel,
            let matchedBarcode,
            let trackCount
        ):
            self = .conflict(
                discidResults: discidResults,
                discidLibraryStatuses: kept(discidLibraryStatuses),
                barcodeResults: barcodeResults,
                barcodeLibraryStatuses: kept(barcodeLibraryStatuses),
                discidSourceLabel: discidSourceLabel,
                matchedBarcode: matchedBarcode,
                trackCount: trackCount,
            )
        case .idle, .triangulating, .notFoundAnywhere, .manualOnly:
            break
        }
    }
}
