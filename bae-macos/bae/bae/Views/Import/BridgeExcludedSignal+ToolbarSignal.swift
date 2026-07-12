import BaeKit

extension BridgeExcludedSignal {
    /// The exclusion that toggles `signal` — its kind decides the variant; a
    /// catalog carries its value. Returns `nil` for a catalog with no value
    /// (which can't be toggled meaningfully).
    init?(signal: BridgeToolbarSignal) {
        switch signal.kind {
        case .discId: self = .disc
        case .barcode: self = .barcode
        case .catalog:
            guard let value = signal.value else {
                return nil
            }
            self = .catalog(value: value)
        }
    }
}
