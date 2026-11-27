import BaeKit
import Foundation

/// The text pools extracted from one candidate's files, which feed the search
/// form's autocomplete.
struct Signals: Equatable {
    let text: BridgeTextSignal

    init(bridge: BridgeSignals) {
        text = bridge.text
    }
}

extension BridgeTextSignal {
    /// The catalog numbers, for the catalog-search autocomplete.
    var catalogValues: [String] {
        switch self {
        case .scanning(let catalogs, _),
            .settled(let catalogs, _):
            catalogs
        case .failed:
            []
        }
    }

    var freeText: [String] {
        switch self {
        case .scanning(_, let freeText),
            .settled(_, let freeText):
            freeText
        case .failed:
            []
        }
    }

    /// How bae broke reading the text off the folder, where it did.
    var failure: BridgeInternalFailure? {
        if case .failed(let failure) = self {
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
