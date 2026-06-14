import Foundation

/// The text pools extracted from a candidate's files, mirrored from
/// `bae_core::signals::Signals` (via `BridgeSignals`). Carried per-candidate;
/// `nil` until the first extraction snapshot arrives. The search form feeds
/// these into the autocomplete fields. The disc-ID and barcode signals reach
/// the UI through the interactive toolbar (`SignalsToolbar`), not here, so
/// only the `text` pools are mirrored.

/// The classified-text signal. Mirrors `bae_core::signals::TextSignal`. Only
/// the catalog / free-text *values* feed the autocomplete here; catalog
/// origins reach the UI through the toolbar badges.
enum TextSignal: Equatable {
    case scanning(catalogs: [String], freeText: [String])
    case settled(catalogs: [String], freeText: [String])

    init(bridge: BridgeTextSignal) {
        switch bridge {
        case .scanning(let catalogs, let freeText):
            self = .scanning(
                catalogs: catalogs.map(\.value),
                freeText: freeText
            )
        case .settled(let catalogs, let freeText):
            self = .settled(catalogs: catalogs.map(\.value), freeText: freeText)
        }
    }

    /// The catalog-number strings — for the catalog-search autocomplete.
    var catalogValues: [String] {
        switch self {
        case .scanning(let catalogs, _), .settled(let catalogs, _): catalogs
        }
    }

    var freeText: [String] {
        switch self {
        case .scanning(_, let freeText), .settled(_, let freeText): freeText
        }
    }

    var isScanning: Bool {
        if case .scanning = self {
            return true
        }
        return false
    }
}

/// The text pools extracted from one candidate's files. Mirrors the `text`
/// slice of `bae_core::signals::Signals`.
struct Signals: Equatable {
    let text: TextSignal

    init(text: TextSignal) {
        self.text = text
    }

    init(bridge: BridgeSignals) {
        text = TextSignal(bridge: bridge.text)
    }
}
