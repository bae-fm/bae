import Foundation

/// Resolve a bae-core message key against the generated `Core` string table.
/// One source of the table lookup for the error/playback lines below.
private func localizedCoreString(_ key: String) -> String {
    NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
}

extension BridgeErrorCategory {
    /// The generic, user-facing line for this category, resolved from the
    /// generated `Core` string table via the key bae-core owns. The underlying
    /// Rust error chain travels separately as opaque `detail` and is never
    /// translated.
    var localizedLine: String {
        localizedCoreString(bridgeErrorCategoryKey(category: self))
    }
}

extension BridgeEntityKind {
    /// The "… not found" line for this entity, resolved from the generated
    /// `Core` string table via the key bae-core owns.
    var notFoundLine: String {
        localizedCoreString(bridgeEntityNotFoundKey(entity: self))
    }
}

extension BridgeError {
    /// The localized, user-facing line for this error. `Cancelled` shows
    /// nothing (the user dismissed it themselves).
    var localizedLine: String {
        switch self {
        case .Cancelled:
            return ""
        case .NotFound(let entity, _):
            return entity.notFoundLine
        case .Diagnostic(let category, _):
            return category.localizedLine
        }
    }

    /// The opaque Rust error chain, for logs and a copyable disclosure. Present
    /// only on `Diagnostic`; never translated. `NotFound`/`Cancelled` carry no
    /// detail to surface.
    var detail: String? {
        switch self {
        case .Diagnostic(_, let detail):
            return detail
        case .NotFound, .Cancelled:
            return nil
        }
    }
}

extension BridgePlaybackErrorReason {
    /// The localized, user-facing line for this playback failure. The two
    /// actionable cloud-only cases resolve their own keyed line; everything
    /// else renders through the shared `BridgeError` path.
    var localizedLine: String {
        switch self {
        case .syncDisconnected, .uploadPending:
            return localizedCoreString(
                bridgePlaybackErrorReasonKey(reason: self)!
            )
        case .diagnostic(let error):
            return error.localizedLine
        }
    }

    /// The opaque, log-only detail — present only on the diagnostic arm, where
    /// it rides on the composed `BridgeError`.
    var detail: String? {
        switch self {
        case .diagnostic(let error):
            return error.detail
        case .syncDisconnected, .uploadPending:
            return nil
        }
    }
}
