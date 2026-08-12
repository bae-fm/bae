import BaeKit
import Foundation

enum LibraryRemovalConfirmation {
    /// A synced library's cloud copy survives; queued cloud writes are lost. A
    /// never-synced library's catalog is gone, while indexed audio files stay
    /// in the user's folders. Pending work is irrelevant without a cloud home.
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
                "Your library in the cloud is untouched — you can restore it from the welcome screen later."
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
            hasCloudHome: library.cloudProvider != nil,
            hasPendingCloudWork: false
        )
    }
}
