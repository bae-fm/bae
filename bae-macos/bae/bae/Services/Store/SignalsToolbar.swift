import BaeKit
import Foundation
import os.log

private let logger = Logger.bae("SignalsToolbar")

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

/// Why a metadata lookup failed. Mirrors `bae_core::signals::LookupFailure`
/// (via `BridgeLookupFailure`). The locale never crosses the bridge: core
/// emits the typed reason and a stable catalog key; this resolves the
/// localized line, rendering `provider`'s status as the message argument.
/// `diagnostic` carries opaque, log-only detail — never primary copy.
enum LookupFailure: Equatable {
    case network
    case provider(status: UInt16?)
    case timeout
    case artworkAnalysis
    case diagnostic(detail: String)

    init(bridge: BridgeLookupFailure) {
        switch bridge {
        case .network: self = .network
        case .provider(let status): self = .provider(status: status)
        case .timeout: self = .timeout
        case .artworkAnalysis: self = .artworkAnalysis
        case .diagnostic(let detail): self = .diagnostic(detail: detail)
        }
    }

    /// The bridge value this wraps — for resolving the catalog key core owns.
    private var bridge: BridgeLookupFailure {
        switch self {
        case .network: .network
        case .provider(let status): .provider(status: status)
        case .timeout: .timeout
        case .artworkAnalysis: .artworkAnalysis
        case .diagnostic(let detail): .diagnostic(detail: detail)
        }
    }

    /// The localized one-line reason. `provider` formats the HTTP status into
    /// the message (or a no-status fallback when absent); `diagnostic` has no
    /// translated copy, so it renders a generic line — the opaque detail is
    /// shown separately (`diagnosticDetail`), never as primary copy.
    var localizedDescription: String {
        switch self {
        case .diagnostic:
            // No translated copy — the opaque detail is shown separately.
            return localizedCore("core.lookup.failure.diagnostic")
        case .network, .timeout, .provider, .artworkAnalysis:
            // Core owns the key for every typed variant (including the
            // status-vs-no-status split for `provider`); the UI never picks it.
            guard let key = bridgeLookupFailureKey(failure: bridge) else {
                logger.warning(
                    "no catalog key for lookup failure \(String(reflecting: self))"
                )
                return localizedCore("core.lookup.failure.diagnostic")
            }
            let format = localizedCore(key)
            if case .provider(let status) = self, let status {
                return String(format: format, NSNumber(value: status).intValue)
            }
            return format
        }
    }

    /// The opaque, log-only detail for a `diagnostic` failure — shown as
    /// secondary text, never translated. `nil` for the typed variants.
    var diagnosticDetail: String? {
        if case .diagnostic(let detail) = self {
            return detail
        }
        return nil
    }

    /// The full badge line: the localized reason, with the opaque diagnostic
    /// detail appended (so a local failure still surfaces its cause, never as
    /// primary copy). The typed variants render their reason alone.
    var badgeLine: String {
        if let detail = diagnosticDetail {
            return "\(localizedDescription): \(detail)"
        }
        return localizedDescription
    }
}

/// Resolve a `core.*` catalog key through the app's String Catalog.
private func localizedCore(_ key: String) -> String {
    NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
}

/// The live lookup/match state of one badge. Mirrors
/// `bae_core::identify::SignalState`.
enum SignalState: Equatable {
    case lookingUp
    case found(count: UInt32)
    case noMatch
    case skipped
    case failed(failure: LookupFailure)
    case confirms(count: UInt32)

    init(bridge: BridgeSignalState) {
        switch bridge {
        case .lookingUp: self = .lookingUp
        case .found(let count): self = .found(count: count)
        case .noMatch: self = .noMatch
        case .skipped: self = .skipped
        case .failed(let failure):
            self = .failed(failure: LookupFailure(bridge: failure))
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
