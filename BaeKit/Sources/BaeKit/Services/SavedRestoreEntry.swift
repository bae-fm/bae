/// A restore code coven saved in iCloud Keychain, with the details read off it
/// once. A code carries no name, so its provider and storage location are what
/// a list shows for it.
public struct SavedRestoreEntry {
    public let code: BridgeSavedRestoreCode
    public let info: BridgeSavedRestoreCodeInfo

    public init(code: BridgeSavedRestoreCode) {
        self.code = code
        info = code.info()
    }
}
