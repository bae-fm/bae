import Foundation
import os.log

private let logger = Logger.bae("ReleaseFilesReader")

/// A live read of one release's files in one order: the release and the
/// order change in place, and each value names the request it answers.
public struct ReleaseFilesQuery: Sendable {
    public let setRequest: @Sendable (BridgeReleaseFilesRequest?) throws -> Void
    public let values: PulledValues<BridgeReleaseFilesSnapshot>

    public init(
        setRequest:
            @escaping @Sendable (BridgeReleaseFilesRequest?) throws -> Void,
        values: PulledValues<BridgeReleaseFilesSnapshot>
    ) {
        self.setRequest = setRequest
        self.values = values
    }

    #if os(macOS)
        init(_ subscription: ReleaseFilesSubscription) {
            self.init(
                setRequest: { try subscription.setRequest(request: $0) },
                values: PulledValues(subscription)
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
    private var deliveries:
        ScreenSubscription<PulledValues<BridgeReleaseFilesSnapshot>>?
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
        deliveries?.stop()
        deliveries = nil
        query = nil
    }

    private func openRead() -> ReleaseFilesQuery {
        let query = open()
        self.query = query
        deliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] snapshot in
                guard let self, let answered = snapshot.request,
                    answered == self.request
                else { return }
                self.onFiles(snapshot.files)
            },
            onError: { [weak self] error in self?.readFailed(error) }
        )
        return query
    }

    private func readFailed(_ error: any Error) {
        if request != nil {
            onError(error)
        }
        else {
            logger.warning(
                "release files read failed while showing nothing: \(String(reflecting: error))"
            )
        }
    }
}
