import Observation
import os.log

private let logger = Logger.bae("LibraryRemovals")

/// Removing libraries from this device, the question a removal asks when this
/// device has changes it hasn't sent, and the one removal that left the store
/// but couldn't delete this device's preferences for it. Process-wide, so a
/// removal started in Settings and one started where libraries are listed
/// finish in the same place.
@MainActor
@Observable
public final class LibraryRemovals {
    /// The library whose removal is running from where libraries are listed.
    public private(set) var removingLibraryId: String?
    /// A removal this device's unsent changes stopped; nothing was removed
    /// until the person removes the library anyway or keeps it.
    public private(set) var unsent: UnsentRemoval?
    /// A removal whose preferences remain, with why; its retry keeps what it
    /// needs to finish.
    public private(set) var unfinished: BridgeRemovalRetry?
    /// The last removal failure, or why `unfinished` stopped.
    public private(set) var error: DisplayError?
    public private(set) var isRetrying = false

    @ObservationIgnored
    private let removeClosed:
        @Sendable (String, BridgeIfUnsent) async throws ->
            BridgeLibraryRemoval
    @ObservationIgnored
    private var task: Task<Void, Never>?

    public init(
        removeClosed:
            @escaping @Sendable (String, BridgeIfUnsent) async throws ->
            BridgeLibraryRemoval
    ) {
        self.removeClosed = removeClosed
    }

    /// The production wiring over the process's host.
    public static func live(host: BridgeHost) -> LibraryRemovals {
        LibraryRemovals { libraryId, unsent in
            try await removeClosedLibrary(libraryId, unsent: unsent, host: host)
        }
    }

    /// Leaving the store runs on an open handle, so the library opens first.
    /// A removal that left the library in place — refused for unsent changes
    /// or failed — closes the handle it opened.
    nonisolated private static func removeClosedLibrary(
        _ libraryId: String,
        unsent: BridgeIfUnsent,
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
        let removal: BridgeLibraryRemoval
        do {
            removal = try await app.removeLibrary(unsent: unsent)
        }
        catch {
            await close(app, libraryId: libraryId)
            throw error
        }
        if case .unsent = removal {
            await close(app, libraryId: libraryId)
        }
        return removal
    }

    nonisolated private static func close(
        _ app: AppHandle,
        libraryId: String
    ) async {
        do {
            try await DetachedWork.run { try app.closeLibrary() }
        }
        catch {
            logger.error(
                "Couldn't close \(libraryId) after its removal stopped: \(error.localizedDescription)"
            )
        }
    }

    /// Remove a library that isn't open, then report the list changed. Changes
    /// this device hasn't sent stop it first, asking the person through
    /// `unsent`.
    public func remove(
        closedLibrary libraryId: String,
        onFinished: @escaping () -> Void
    ) {
        run(closedLibrary: libraryId, unsent: .refuse, onFinished: onFinished)
    }

    private func run(
        closedLibrary libraryId: String,
        unsent: BridgeIfUnsent,
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
                let removal = try await removeClosed(libraryId, unsent)
                if case .unsent(let writes, let files) = removal {
                    ask(
                        UnsentEdits(writes: writes, files: files),
                        discard: { [weak self] in
                            self?
                                .run(
                                    closedLibrary: libraryId,
                                    unsent: .discard,
                                    onFinished: onFinished
                                )
                        },
                        keep: {}
                    )
                    return
                }
                record(removal)
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

    /// Ask the person about the unsent changes that stopped a removal:
    /// `discard` removes the library anyway, losing them; `keep` keeps it.
    public func ask(
        _ edits: UnsentEdits,
        discard: @escaping @MainActor () -> Void,
        keep: @escaping @MainActor () -> Void
    ) {
        precondition(unsent == nil, "one unsent-changes question at a time")
        unsent = UnsentRemoval(edits: edits, discard: discard, keep: keep)
    }

    /// Remove the library the question is about, losing its unsent changes.
    public func discardUnsent() {
        guard let unsent else {
            preconditionFailure("Remove Anyway shows only while asking")
        }
        self.unsent = nil
        unsent.discard()
    }

    /// Keep the library the question is about.
    public func keepUnsent() {
        guard let unsent else {
            preconditionFailure("Cancel shows only while asking")
        }
        self.unsent = nil
        unsent.keep()
    }

    /// Note how a removal ended: its preferences may remain to be deleted. A
    /// removal stopped for unsent changes is asked about, not recorded.
    public func record(_ removal: BridgeLibraryRemoval) {
        switch removal {
        case .removed:
            unfinished = nil
            error = nil
        case .preferencesRemain(let retry, let failure):
            logger.error("A removed library's preferences remain: \(failure)")
            unfinished = retry
            error = DisplayError(failure)
        case .unsent:
            preconditionFailure(
                "a removal stopped for unsent changes is asked about"
            )
        }
    }

    /// Delete the preferences an unfinished removal left.
    public func retryUnfinished() {
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

/// A removal this device's unsent changes stopped, with what removing anyway
/// and keeping the library each do.
public struct UnsentRemoval {
    public let edits: UnsentEdits
    let discard: @MainActor () -> Void
    let keep: @MainActor () -> Void
}
