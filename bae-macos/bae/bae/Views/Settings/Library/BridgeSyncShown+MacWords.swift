import BaeKit
import Foundation

extension BridgeSyncShown {
    /// Why the state is what it is, in the person's language and this Mac's
    /// terms.
    var causeLine: String? {
        cause?.macLine
    }
}

extension BridgeSyncCause {
    /// Core's words for the cause, but for the three that name the device:
    /// the Mac words those, with the keychain where the saved keys live.
    var macLine: String {
        switch self {
        case .message(let message, let resumesAt):
            message.syncCauseLine(resumesAt: resumesAt)
        case .deviceDiskFull:
            String(
                localized:
                    "There is no space left on this Mac. Free some space and bae will try again.",
                comment:
                    "Sync failed: the Mac's disk is full; bae retries once there is room."
            )
        case .deviceKeysUnreadable:
            String(
                localized: "The keychain is locked. Unlock it to keep syncing.",
                comment:
                    "Sync failed: the keychain holding bae's saved keys can't be read while it is locked."
            )
        case .libraryUnreadable:
            String(
                localized: "bae could not read the library on this Mac.",
                comment:
                    "Sync failed: the library's database on this Mac couldn't be read."
            )
        }
    }
}
