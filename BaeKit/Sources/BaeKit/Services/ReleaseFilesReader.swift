import Foundation
import os.log

private let logger = Logger.bae("ReleaseFilesReader")

/// A live read of one release's files in one order: the release and the
/// order change in place, and each value names the request it answers.
public struct ReleaseFilesQuery: Sendable {
    public let setRequest: @Sendable (BridgeReleaseFilesRequest?) throws -> Void
    public let next: @Sendable () async throws -> BridgeReleaseFilesSnapshot
    public let cancel: @Sendable () async -> Void

    public init(
        setRequest:
            @escaping @Sendable (BridgeReleaseFilesRequest?) throws -> Void,
        next:
            @escaping @Sendable () async throws
            -> BridgeReleaseFilesSnapshot,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setRequest = setRequest
        self.next = next
        self.cancel = cancel
    }

    #if os(macOS)
        init(_ subscription: any ReleaseFilesSubscriptionProtocol) {
            self.init(
                setRequest: { try subscription.setRequest(request: $0) },
                next: { try await subscription.next() },
                cancel: { try? await subscription.cancel() }
            )
        }
    #endif
}

/// One view's read of a release's files, kept for as long as the view is:
/// another order or release moves the read in place. Each value is handed
/// on only while it answers the request shown now.
@MainActor
public final class ReleaseFilesReader {
    private let open: () -> ReleaseFilesQuery
    private let onFiles: @MainActor ([BridgeFile]?) -> Void
    private let onError: @MainActor (any Error) -> Void
    private var query: ReleaseFilesQuery?
    private var deliveries: Task<Void, Never>?
    /// The release and order shown now, or `nil` before the first `show`
    /// and after `close`.
    public private(set) var request: BridgeReleaseFilesRequest?

    /// `onFiles` receives the files in the order asked for, or `nil` once
    /// no such release exists.
    public init(
        open: @escaping () -> ReleaseFilesQuery,
        onFiles: @escaping @MainActor ([BridgeFile]?) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        self.open = open
        self.onFiles = onFiles
        self.onError = onError
    }

    deinit {
        deliveries?.cancel()
        if let query {
            Task { await query.cancel() }
        }
    }

    /// Show `request`, opening the read on first use.
    public func show(_ request: BridgeReleaseFilesRequest) {
        guard request != self.request || query == nil else { return }
        self.request = request
        do {
            try (query ?? openRead()).setRequest(request)
        }
        catch {
            onError(error)
        }
    }

    /// Read the request shown now again on a fresh read, after one failed.
    public func retry() {
        guard let request else { return }
        closeRead()
        show(request)
    }

    /// Stop showing anything and end the read.
    public func close() {
        request = nil
        closeRead()
    }

    private func closeRead() {
        deliveries?.cancel()
        deliveries = nil
        if let query {
            self.query = nil
            Task { await query.cancel() }
        }
    }

    private func openRead() -> ReleaseFilesQuery {
        let query = open()
        self.query = query
        deliveries = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let snapshot = try await query.next()
                    guard let self else { return }
                    guard let answered = snapshot.request,
                        answered == self.request
                    else { continue }
                    self.onFiles(snapshot.files)
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch is CancellationError {
                    return
                }
                catch {
                    guard let self else { return }
                    if self.request != nil {
                        self.onError(error)
                    }
                    else {
                        logger.warning(
                            "release files read failed while showing nothing: \(error.localizedDescription)"
                        )
                    }
                }
            }
        }
        return query
    }
}
