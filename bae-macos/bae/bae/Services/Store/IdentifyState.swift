import Foundation

/// Which signal(s) backed a terminal `Found` state. Drives source-specific
/// banner copy.
enum IdentifySource: Equatable {
    case discid
    case barcode
    /// Both signals contributed to the result via intersection.
    case combined

    init(bridge: BridgeIdentifySource) {
        switch bridge {
        case .discid: self = .discid
        case .barcode: self = .barcode
        case .combined: self = .combined
        }
    }
}

/// Per-signal disc-ID progress inside `Triangulating`.
enum DiscidProgress: Equatable {
    case computing
    case lookingUp
    case done(nResults: UInt32)
    case skipped
    case failed(failure: LookupFailure)

    init(bridge: BridgeDiscidProgress) {
        switch bridge {
        case .computing: self = .computing
        case .lookingUp: self = .lookingUp
        case .done(let n): self = .done(nResults: n)
        case .skipped: self = .skipped
        case .failed(let failure):
            self = .failed(failure: LookupFailure(bridge: failure))
        }
    }
}

/// Per-signal barcode progress inside `Triangulating`.
enum BarcodeProgress: Equatable {
    case scanning
    case lookingUp(current: String, position: UInt32, total: UInt32)
    case done(nResults: UInt32)
    case skipped

    init(bridge: BridgeBarcodeProgress) {
        switch bridge {
        case .scanning: self = .scanning
        case .lookingUp(let current, let position, let total):
            self = .lookingUp(
                current: current,
                position: position,
                total: total,
            )
        case .done(let n): self = .done(nResults: n)
        case .skipped: self = .skipped
        }
    }
}

/// Which signals produced or confirmed one result, for the per-row badges.
/// Mirrors `bae_core::identify::ResultProvenance`.
struct ResultProvenance: Equatable {
    let byDiscId: Bool
    let byBarcode: Bool
    let matchesCatalog: Bool

    init(byDiscId: Bool, byBarcode: Bool, matchesCatalog: Bool) {
        self.byDiscId = byDiscId
        self.byBarcode = byBarcode
        self.matchesCatalog = matchesCatalog
    }

    init(bridge: BridgeResultProvenance) {
        byDiscId = bridge.byDiscId
        byBarcode = bridge.byBarcode
        matchesCatalog = bridge.matchesCatalog
    }
}

/// Mirror of `bae_core::identify::IdentifyState`. The reducer assigns one
/// of these onto a candidate for every bus emission; the UI switches on
/// the variant to render banners and match lists.
enum IdentifyState: Equatable {
    case idle
    /// Both signals running in parallel. Per-signal progress lets the UI
    /// show side-by-side pipes ("Computing disc-id ✓ · Looking up barcode
    /// 2 of 3..."). The pipeline transitions to a terminal state once
    /// both pipes settle.
    case triangulating(discid: DiscidProgress, barcode: BarcodeProgress)
    case found(
        group: ReleaseGroup,
        libraryStatuses: [String: LibraryStatus],
        trackCount: UInt32,
        source: IdentifySource,
        /// Per-pressing provenance keyed by release id — the per-row badges.
        provenance: [String: ResultProvenance],
    )
    /// Signals disagreed: empty intersection or multi-group result. The
    /// conflict surface presents the per-signal sections so the user can
    /// pick a section, exclude a signal, or fall back to manual search.
    case conflict(
        discidResults: [MetadataResult],
        discidLibraryStatuses: [String: LibraryStatus],
        barcodeResults: [MetadataResult],
        barcodeLibraryStatuses: [String: LibraryStatus],
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
            self = .triangulating(
                discid: DiscidProgress(bridge: discid),
                barcode: BarcodeProgress(bridge: barcode),
            )
        case .found(
            let group,
            let libraryStatuses,
            let trackCount,
            let source,
            let provenance
        ):
            self = .found(
                group: ReleaseGroup(bridge: group),
                libraryStatuses: libraryStatuses.mapValues(
                    LibraryStatus.init(bridge:)
                ),
                trackCount: trackCount,
                source: IdentifySource(bridge: source),
                provenance: provenance.mapValues(
                    ResultProvenance.init(bridge:)
                ),
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
                discidResults: discidResults.map(MetadataResult.init(bridge:)),
                discidLibraryStatuses: discidLibraryStatuses.mapValues(
                    LibraryStatus.init(bridge:),
                ),
                barcodeResults: barcodeResults.map(
                    MetadataResult.init(bridge:)
                ),
                barcodeLibraryStatuses: barcodeLibraryStatuses.mapValues(
                    LibraryStatus.init(bridge:),
                ),
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
        where shouldRemove: (String, LibraryStatus) -> Bool
    ) {
        func kept(
            _ statuses: [String: LibraryStatus]
        ) -> [String: LibraryStatus] {
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
