import BaeKit
import Foundation

/// The text pools extracted from one candidate's files, which feed the search
/// form's autocomplete.
struct Signals: Equatable {
    let text: BridgeTextSignal

    init(text: BridgeTextSignal) {
        self.text = text
    }

    init(bridge: BridgeSignals) {
        text = bridge.text
    }
}

extension BridgeTextSignal {
    /// The catalog numbers, for the catalog-search autocomplete.
    var catalogValues: [String] {
        switch self {
        case .scanning(let catalogs, _),
            .settled(let catalogs, _),
            .failed(_, let catalogs, _):
            catalogs
        }
    }

    var freeText: [String] {
        switch self {
        case .scanning(_, let freeText),
            .settled(_, let freeText),
            .failed(_, _, let freeText):
            freeText
        }
    }

    var failure: BridgeLookupFailure? {
        if case .failed(let failure, _, _) = self {
            return failure
        }
        return nil
    }

    var isScanning: Bool {
        if case .scanning = self {
            return true
        }
        return false
    }
}
