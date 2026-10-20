/// One library's entry in iCloud Keychain, as the saved-code list shows it: a
/// code to restore from, or one bae can't read, which the person can only
/// delete. Either is deleted by its library id.
public enum SavedRestoreEntry {
    case readable(SavedRestoreCode)
    case unreadable(libraryId: String, reason: BridgeUnreadableSavedCode)

    public init(_ entry: BridgeSavedRestoreEntry) {
        switch entry {
        case .readable(let code):
            self = .readable(SavedRestoreCode(code: code))
        case .unreadable(let libraryId, let reason):
            self = .unreadable(libraryId: libraryId, reason: reason)
        }
    }

    /// The library the entry is saved under.
    public var libraryId: String {
        switch self {
        case .readable(let saved): saved.info.libraryId
        case .unreadable(let libraryId, _): libraryId
        }
    }
}
