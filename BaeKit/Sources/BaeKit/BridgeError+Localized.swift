import Foundation

/// Resolve a bae-core message key against the generated `Core` string table.
/// One source of the table lookup for the error/playback lines below.
public func localizedCoreString(_ key: String) -> String {
    NSLocalizedString(key, tableName: "Core", bundle: .module, comment: "")
}

extension BridgeErrorCategory {
    /// The generic, user-facing line for this category, resolved from the
    /// generated `Core` string table via the key bae-core owns. The underlying
    /// Rust error chain travels separately as opaque `detail` and is never
    /// translated.
    public var localizedLine: String {
        localizedCoreString(bridgeErrorCategoryKey(category: self))
    }
}

extension BridgeEntityKind {
    /// The "… not found" line for this entity, resolved from the generated
    /// `Core` string table via the key bae-core owns.
    public var notFoundLine: String {
        localizedCoreString(bridgeEntityNotFoundKey(entity: self))
    }
}

extension BridgeError {
    /// The localized, user-facing line for this error, or `nil` when it has none
    /// to show — core's answer, not this app's. A cancellation is the user's own
    /// doing and says nothing back to them.
    ///
    /// `nil`, never `""`: an empty string is not "nothing", it is a line that
    /// happens to be blank, and it used to open the error alert with an empty
    /// message.
    public var localizedLine: String? {
        bridgeErrorLineKey(error: self).map(localizedCoreString)
    }

    /// The opaque Rust error chain, for logs and a copyable disclosure. Present
    /// only on `Diagnostic`; never translated. `NotFound`/`Cancelled` carry no
    /// detail to surface.
    public var detail: String? {
        switch self {
        case .Diagnostic(_, let detail):
            return detail
        case .NotFound, .Cancelled:
            return nil
        }
    }
}

extension BridgePlaybackErrorReason {
    /// The localized, user-facing line for this playback failure, or `nil` when
    /// there is none. The two actionable cloud-only cases resolve their own keyed
    /// line; everything else renders through the shared `BridgeError` path, which
    /// is also where "no line at all" comes from.
    public var localizedLine: String? {
        switch self {
        case .syncDisconnected, .uploadPending:
            return bridgePlaybackErrorReasonKey(reason: self)
                .map(localizedCoreString)
        case .diagnostic(let error):
            return error.localizedLine
        }
    }

    /// The opaque, log-only detail — present only on the diagnostic arm, where
    /// it rides on the composed `BridgeError`.
    public var detail: String? {
        switch self {
        case .diagnostic(let error):
            return error.detail
        case .syncDisconnected, .uploadPending:
            return nil
        }
    }
}
