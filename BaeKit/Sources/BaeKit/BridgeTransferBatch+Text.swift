import Foundation

extension BridgeTransferBatch {
    /// How many of a transfer line's releases are done, of all its batch
    /// holds ("1 of 3 releases"), from core's catalog.
    public var text: String {
        localizedCoreString(
            "core.activity.releases_done",
            Int64(done),
            Int64(total)
        )
    }
}
