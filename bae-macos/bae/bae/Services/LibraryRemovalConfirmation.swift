import BaeKit
import Foundation

enum LibraryRemovalConfirmation {
    /// A synced library's cloud copy survives, and this Mac leaves its members
    /// first, which needs storage; changes it hasn't sent are counted when the
    /// removal stops for them. A never-synced library's catalog is gone, while
    /// indexed audio files stay in the user's folders.
    static func message(hasCloudHome: Bool) -> String {
        guard hasCloudHome else {
            return String(
                localized:
                    "This library has never been synced. Its catalog will be permanently deleted; audio files in your folders aren't deleted."
            )
        }
        return String(
            localized:
                "This Mac leaves the library, then its copy is deleted. Removing needs a connection to your storage; your library in the cloud is untouched."
        )
    }

    static func message(for library: BridgeLibrary) -> String {
        message(hasCloudHome: hasCloudHome(library.settings))
    }

    /// Settings that can't be read may name a cloud home, so the warning
    /// assumes one.
    private static func hasCloudHome(_ settings: BridgeLibrarySettings) -> Bool
    {
        if case .readable(storage: nil) = settings { return false }
        return true
    }
}
