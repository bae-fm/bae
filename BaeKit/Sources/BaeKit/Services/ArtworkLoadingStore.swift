import Foundation
import Observation

/// The post-open CacheEager artwork operation for one library. The bridge is
/// its sole writer; views read the retained status and may request cancellation.
@MainActor
@Observable
public final class ArtworkLoadingStore {
    public private(set) var status: BridgeEagerCacheFillStatus = .notRunning
    private let cancelAction: @Sendable () -> Void

    public init(cancel: @escaping @Sendable () -> Void) {
        self.cancelAction = cancel
    }

    public func apply(_ status: BridgeEagerCacheFillStatus) {
        self.status = status
    }

    public func cancel() {
        cancelAction()
    }
}
