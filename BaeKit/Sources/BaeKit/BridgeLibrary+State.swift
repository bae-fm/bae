import Foundation

extension BridgeLibrary {
    /// The name the library is listed under: its own, or for one whose name
    /// can't be read, a fixed title.
    public var title: String {
        name ?? localizedCoreString("core.library.name_unreadable")
    }

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
        case .nameUnreadable:
            return id
        case .unreadable(let error):
            guard let line = error.localizedLine else {
                preconditionFailure(
                    "unreadable settings are a diagnostic, never a cancellation"
                )
            }
            return line
        case .replacementUnfinished(_, let lineKey),
            .deletionUnfinished(let lineKey):
            return localizedCoreString(lineKey)
        }
    }

    /// Whether Reset from cloud is what gets past `failure`, a failed open of
    /// this library: core says the failure is in this device's copy of the
    /// library's data, and the library has storage to reset from. A library
    /// kept on this device only has none, which coven refuses a reset for
    /// (`NoStorage`), and one that can't open as it is has no storage the app
    /// knows of.
    public func offersResetFromCloud(after failure: DisplayError) -> Bool {
        guard failure.resetFromCloudReplaces,
            case .openable(let storage) = state
        else { return false }
        return storage != nil
    }
}
