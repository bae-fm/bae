import Foundation

/// The interactive signals toolbar, mirrored from
/// `bae_core::identify::ToolbarSignal` (via `BridgeSignalsToolbar`). Each
/// identifying signal — the disc ID, the barcode, and every catalog candidate
/// — is one `ToolbarSignal` badge the view renders directly: value, origin,
/// live state, and whether the user excluded it. Core pre-shapes the whole
/// list; the view iterates and renders.

/// Where a signal value was harvested from — what a badge shows on hover.
/// Mirrors `bae_core::signals::SignalOrigin`.
enum SignalOrigin: Equatable {
    case discToc
    case cueSheet
    case artwork
    case folderName
    case filename
    case textFile

    init(bridge: BridgeSignalOrigin) {
        switch bridge {
        case .discToc: self = .discToc
        case .cueSheet: self = .cueSheet
        case .artwork: self = .artwork
        case .folderName: self = .folderName
        case .filename: self = .filename
        case .textFile: self = .textFile
        }
    }
}

/// Which kind of signal a badge represents. Mirrors
/// `bae_core::identify::SignalKind`.
enum SignalKind: Equatable {
    case discId
    case barcode
    case catalog

    init(bridge: BridgeSignalKind) {
        switch bridge {
        case .discId: self = .discId
        case .barcode: self = .barcode
        case .catalog: self = .catalog
        }
    }
}

/// A badge's role in triangulation. Mirrors `bae_core::identify::SignalRole`.
enum SignalRole: Equatable {
    case identity
    case filter

    init(bridge: BridgeSignalRole) {
        switch bridge {
        case .identity: self = .identity
        case .filter: self = .filter
        }
    }
}

/// The live lookup/match state of one badge. Mirrors
/// `bae_core::identify::SignalState`.
enum SignalState: Equatable {
    case lookingUp
    case found(count: UInt32)
    case noMatch
    case skipped
    case failed(message: String)
    case confirms(count: UInt32)

    init(bridge: BridgeSignalState) {
        switch bridge {
        case .lookingUp: self = .lookingUp
        case .found(let count): self = .found(count: count)
        case .noMatch: self = .noMatch
        case .skipped: self = .skipped
        case .failed(let message): self = .failed(message: message)
        case .confirms(let count): self = .confirms(count: count)
        }
    }
}

/// One badge in the signals toolbar. Mirrors
/// `bae_core::identify::ToolbarSignal`.
struct ToolbarSignal: Equatable, Identifiable {
    let kind: SignalKind
    let role: SignalRole
    let value: String?
    let origin: SignalOrigin
    let state: SignalState
    let excluded: Bool

    init(
        kind: SignalKind,
        role: SignalRole,
        value: String?,
        origin: SignalOrigin,
        state: SignalState,
        excluded: Bool
    ) {
        self.kind = kind
        self.role = role
        self.value = value
        self.origin = origin
        self.state = state
        self.excluded = excluded
    }

    init(bridge: BridgeToolbarSignal) {
        kind = SignalKind(bridge: bridge.kind)
        role = SignalRole(bridge: bridge.role)
        value = bridge.value
        origin = SignalOrigin(bridge: bridge.origin)
        state = SignalState(bridge: bridge.state)
        excluded = bridge.excluded
    }

    /// Stable identity for `ForEach`. The disc ID and barcode are singletons;
    /// a catalog is keyed by its value. Keeping the row in the tree across
    /// toggles (rather than removing it) preserves layout stability.
    var id: String {
        switch kind {
        case .discId: "disc"
        case .barcode: "barcode"
        case .catalog: "catalog:\(value ?? "")"
        }
    }
}

/// The candidate's full signals toolbar — the ordered badge list. Mirrors
/// `BridgeSignalsToolbar`.
struct SignalsToolbar: Equatable {
    let signals: [ToolbarSignal]

    init(signals: [ToolbarSignal]) {
        self.signals = signals
    }

    init(bridge: BridgeSignalsToolbar) {
        signals = bridge.signals.map(ToolbarSignal.init(bridge:))
    }

    /// The identity badges (disc ID, barcode) — rendered in the left zone.
    var identity: [ToolbarSignal] {
        signals.filter { $0.role == .identity }
    }

    /// The catalog filter badges — rendered in the Refine zone.
    var filters: [ToolbarSignal] {
        signals.filter { $0.role == .filter }
    }
}

/// A signal the user toggled off in the toolbar. Mirrors
/// `bae_core::identify::ExcludedSignal` (and `BridgeExcludedSignal`). Built
/// from a `ToolbarSignal` the user clicked.
enum ExcludedSignal: Equatable {
    case disc
    case barcode
    case catalog(value: String)

    /// The exclusion that toggles `signal` — its kind decides the variant; a
    /// catalog carries its value. Returns `nil` for a catalog with no value
    /// (which can't be toggled meaningfully).
    init?(signal: ToolbarSignal) {
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

    var bridge: BridgeExcludedSignal {
        switch self {
        case .disc: .disc
        case .barcode: .barcode
        case .catalog(let value): .catalog(value: value)
        }
    }
}
