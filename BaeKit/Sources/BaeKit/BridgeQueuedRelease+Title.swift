import Foundation

extension BridgeQueuedRelease {
    /// A queue row's title: the release's album title, or core's line for a
    /// release the library no longer holds.
    public static func titleText(_ release: BridgeQueuedRelease?) -> String {
        release?.title ?? localizedCoreString("core.queue.release_missing")
    }
}
