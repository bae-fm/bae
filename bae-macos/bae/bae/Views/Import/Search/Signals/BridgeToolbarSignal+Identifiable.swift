import BaeKit

extension BridgeToolbarSignal: Identifiable {
    /// Stable identity for `ForEach`. The disc ID and barcode are singletons;
    /// a catalog is keyed by its value. Keeping the row in the tree across
    /// toggles (rather than removing it) preserves layout stability.
    public var id: String {
        switch kind {
        case .discId: "disc"
        case .barcode: "barcode"
        case .catalog: "catalog:\(value ?? "")"
        }
    }
}
