import Foundation

extension BridgeLibrary {
    /// Whether the library opens.
    public var isOpenable: Bool {
        if case .openable = state { return true }
        return false
    }

    /// The line under the library's name: its provider and account when it
    /// has a cloud home, nothing when it doesn't, why its settings can't be
    /// read, or which reset or restore of this device's copy didn't finish.
    public var detailLine: String {
        switch state {
        case .openable(let storage):
            return storage?.accountLine ?? ""
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
        }
    }
}
