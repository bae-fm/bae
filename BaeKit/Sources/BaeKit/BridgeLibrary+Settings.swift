import Foundation

extension BridgeLibrary {
    /// Whether the library's settings can be read, so it can open.
    public var isOpenable: Bool {
        if case .readable = settings { return true }
        return false
    }

    /// The line under the library's name: its provider and account when it
    /// has a cloud home, nothing when it doesn't, or why its settings can't be
    /// read.
    public var detailLine: String {
        switch settings {
        case .readable(let storage):
            return storage?.accountLine ?? ""
        case .unreadable(let error):
            guard let line = error.localizedLine else {
                preconditionFailure(
                    "unreadable settings are a diagnostic, never a cancellation"
                )
            }
            return line
        }
    }
}
