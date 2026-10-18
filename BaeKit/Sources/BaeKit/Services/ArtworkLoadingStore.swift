import Foundation
import Observation

@MainActor
@Observable
public final class ArtworkLoadingStore {
    public private(set) var status: BridgeEagerCacheFillStatus = .idle
    public private(set) var error: DisplayError?
    private let cancelAction: @Sendable () async throws -> Void
    public init(cancel: @escaping @Sendable () async throws -> Void) {
        cancelAction = cancel
    }
    public func apply(_ status: BridgeEagerCacheFillStatus) {
        self.status = status
    }
    public func fail(_ error: any Error) { self.error = DisplayError(error) }
    public func cancel() async {
        do { try await cancelAction() }
        catch { fail(error) }
    }
}
