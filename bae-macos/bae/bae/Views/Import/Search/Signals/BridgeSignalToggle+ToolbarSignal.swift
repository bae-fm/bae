import BaeKit

extension BridgeSignalToggle {
    /// The toggle that acts on `signal` — its kind decides the variant. The
    /// catalog is chosen by value rather than toggled as a whole, so it is not
    /// one of these; `CatalogSignalBadge` sends its own.
    init?(signal: BridgeToolbarSignal) {
        switch signal.kind {
        case .discId: self = .disc
        case .barcode: self = .barcode
        case .catalog: return nil
        }
    }
}
