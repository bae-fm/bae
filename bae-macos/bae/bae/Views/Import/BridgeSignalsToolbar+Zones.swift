import BaeKit

extension BridgeSignalsToolbar {
    /// The identity badges (disc ID, barcode) — rendered in the left zone.
    var identity: [BridgeToolbarSignal] {
        signals.filter { $0.role == .identity }
    }

    /// The catalog filter badges — rendered in the Refine zone.
    var filters: [BridgeToolbarSignal] {
        signals.filter { $0.role == .filter }
    }
}
