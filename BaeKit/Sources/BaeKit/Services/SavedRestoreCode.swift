/// A restore code coven saved in iCloud Keychain that bae can restore from,
/// with the details read off it once: the library's name as one of the
/// person's devices last saw it, and the provider and storage location the
/// code holds.
public struct SavedRestoreCode {
    public let code: BridgeSavedRestoreCode
    public let info: BridgeSavedRestoreCodeInfo

    public init(code: BridgeSavedRestoreCode) {
        self.code = code
        info = code.info()
    }
}
