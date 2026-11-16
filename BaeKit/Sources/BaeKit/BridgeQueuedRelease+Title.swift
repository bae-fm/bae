import Foundation

extension BridgeQueuedRelease {
    /// A queue row's title: the release's album title, or the localized
    /// core.queue.release_missing message when the library no longer holds
    /// the release.
    public static func titleText(_ release: BridgeQueuedRelease?) -> String {
        release?.title ?? localizedCoreString("core.queue.release_missing")
    }
}
