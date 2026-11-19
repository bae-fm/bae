import BaeKit
import Foundation

extension BridgeSyncShown {
    /// Why the state is what it is, in the person's language and this Mac's
    /// terms.
    var causeLine: String? {
        cause?.macLine
    }

    /// The line under the cause that counts down to when sync resumes, when
    /// the cause names that moment.
    var causeCountdown: BridgeSyncCountdown? {
        guard case .message(_, let countdown) = cause else { return nil }
        return countdown
    }
}

extension BridgeSyncCause {
    /// Core's words for the cause, but for those that name the device: the
    /// Mac words those, with the keychain where the saved keys live.
    var macLine: String {
        switch self {
        case .message(let message, _):
            message.localizedLine
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
        case .enterNewRestoreCode:
            String(
                localized:
                    "Enter the new restore code to restore the library on this Mac. The restore discards any changes this Mac has not sent.",
                comment:
                    "Sync status line after another of the person's devices made a new restore code: entering it restores the library on this Mac, and the restore loses what this Mac had not sent."
            )
        case .changesStayOnDevice:
            String(
                localized: "Your changes stay on this Mac.",
                comment:
                    "Sync status line while S3 storage refuses this Mac's access key: what this Mac changed waits here until the person signs in again."
            )
        }
    }
}
