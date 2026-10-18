import BaeKit
import Foundation

enum LibraryRemovalConfirmation {
    /// A synced library's cloud copy survives, and this Mac leaves its members
    /// first, which needs storage; queued cloud writes are lost. A never-synced
    /// library's catalog is gone, while indexed audio files stay in the user's
    /// folders. Pending work is irrelevant without a cloud home.
    static func message(
        hasCloudHome: Bool,
        hasPendingCloudWork: Bool
    ) -> String {
        guard hasCloudHome else {
            return String(
                localized:
                    "This library has never been synced. Its catalog will be permanently deleted; audio files in your folders aren't deleted."
            )
        }
        let base = String(
            localized:
                "This Mac leaves the library, then its copy is deleted. Removing needs a connection to your storage; your library in the cloud is untouched."
        )
        guard hasPendingCloudWork else { return base }
        let extra = String(
            localized:
                "Some changes haven't finished uploading and will be lost."
        )
        return "\(base) \(extra)"
    }

    static func message(for library: BridgeLibrary) -> String {
        message(
            hasCloudHome: library.storage != nil,
            hasPendingCloudWork: false
        )
    }
}
