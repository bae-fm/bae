import Foundation

extension BridgeLibrary {
    /// Whether the library opens.
    public var isOpenable: Bool {
        if case .openable = state { return true }
        return false
    }

    /// What keeps the library from opening as it is, for the line under its
    /// name; nil for one that opens.
    public var stateLine: String? {
        switch state {
        case .openable:
            return nil
        case .unreadable(let error):
            guard let line = error.localizedLine else {
                preconditionFailure(
                    "unreadable settings are a diagnostic, never a cancellation"
                )
            }
            return line
        case .replacementUnfinished(let by):
            return BridgeErrorCategory.replacementUnfinished(by: by)
                .localizedLine
        case .deletionUnfinished:
            return BridgeErrorCategory.deletionUnfinished.localizedLine
        }
    }
}
