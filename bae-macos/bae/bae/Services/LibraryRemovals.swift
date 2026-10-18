import BaeKit
import Observation
import os.log

private let logger = Logger.bae("LibraryRemovals")

/// Removing libraries from this Mac, and the one removal that left the store
/// but couldn't delete this Mac's preferences for it. Process-wide, so a
/// removal started in Settings and one started from the welcome screen finish
/// in the same place.
@MainActor
@Observable
final class LibraryRemovals {
    /// The library whose removal is running from the welcome screen.
    private(set) var removingLibraryId: String?
    /// A removal whose preferences remain, with why; its retry keeps what it
    /// needs to finish.
    private(set) var unfinished: BridgeRemovalRetry?
    /// The last removal failure, or why `unfinished` stopped.
    private(set) var error: DisplayError?
    private(set) var isRetrying = false

    @ObservationIgnored
    private let removeClosed:
        @Sendable (String) async throws ->
            BridgeLibraryRemoval
    @ObservationIgnored
    private var task: Task<Void, Never>?

    init(
        removeClosed:
            @escaping @Sendable (String) async throws -> BridgeLibraryRemoval
    ) {
        self.removeClosed = removeClosed
    }

    /// The production wiring over the process's host.
    static func live(host: BridgeHost) -> LibraryRemovals {
        LibraryRemovals { libraryId in
            try await removeClosedLibrary(libraryId, host: host)
        }
    }

    /// Leaving the store runs on an open handle, so the library opens first.
    /// A refused removal leaves the local copy in place and closes the handle
    /// it opened.
    nonisolated private static func removeClosedLibrary(
        _ libraryId: String,
        host: BridgeHost
    ) async throws -> BridgeLibraryRemoval {
        let app = try await DetachedWork.run {
            try initApp(
                libraryId: libraryId,
                positionUpdateIntervalMs: 200,
                restorePlayback: false,
                host: host
            )
        }
        do {
            return try await app.removeLibrary(unsent: .refuse)
        }
        catch {
            do {
                try await DetachedWork.run { try app.closeLibrary() }
            }
            catch let closeError {
                logger.error(
                    "Couldn't close \(libraryId) after its removal failed: \(closeError.localizedDescription)"
                )
            }
            throw error
        }
    }

    /// Remove a library that isn't open, then report the list changed.
    func remove(
        closedLibrary libraryId: String,
        onFinished: @escaping () -> Void
    ) {
        precondition(task == nil, "one library removal at a time")
        removingLibraryId = libraryId
        error = nil
        task = Task {
            defer {
                removingLibraryId = nil
                task = nil
            }
            do {
                record(try await removeClosed(libraryId))
            }
            catch {
                logger.error(
                    "Couldn't remove \(libraryId): \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
            onFinished()
        }
    }

    /// Note how a removal ended: its preferences may remain to be deleted.
    func record(_ removal: BridgeLibraryRemoval) {
        switch removal {
        case .removed:
            unfinished = nil
            error = nil
        case .preferencesRemain(let retry, let failure):
            logger.error("A removed library's preferences remain: \(failure)")
            unfinished = retry
            error = DisplayError(failure)
        }
    }

    /// Delete the preferences an unfinished removal left.
    func retryUnfinished() {
        guard let retry = unfinished else {
            preconditionFailure("Retry shows only for an unfinished removal")
        }
        precondition(task == nil, "one library removal at a time")
        error = nil
        isRetrying = true
        task = Task {
            defer {
                isRetrying = false
                task = nil
            }
            do {
                try await retry.retry()
                unfinished = nil
            }
            catch {
                logger.error(
                    "Couldn't finish removing a library: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
