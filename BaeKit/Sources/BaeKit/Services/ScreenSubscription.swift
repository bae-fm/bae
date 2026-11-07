import Foundation
import os.log

private let logger = Logger.bae("ScreenSubscription")

/// A bridge object a screen pulls its values from: each `next()` answers
/// with the next value core delivers, and `cancel()` ends it.
public protocol PulledSubscription: Sendable {
    associatedtype Value: Sendable
    func next() async throws -> Value
    func cancel() async throws
}

/// The one way a screen reads a subscription: pull each value and hand it to
/// the screen on the main actor, until `stop()` or until the subscription
/// ends. A failed read is handed to `onError` and the pulls go on: the next
/// value answers the next read that succeeds. Once stopped, nothing more is
/// handed on, even a value whose pull was already under way.
public final class ScreenSubscription<Source: PulledSubscription>: Sendable {
    private let source: Source
    private let pulls: Task<Void, Never>

    public init(
        _ source: Source,
        onValue: @escaping @MainActor @Sendable (Source.Value) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.source = source
        pulls = Task { @MainActor in
            while !Task.isCancelled {
                do {
                    let value = try await source.next()
                    if Task.isCancelled { return }
                    onValue(value)
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch is CancellationError {
                    return
                }
                catch {
                    if Task.isCancelled { return }
                    onError(error)
                }
            }
        }
    }

    deinit {
        pulls.cancel()
        let source = source
        Task { await Self.end(source) }
    }

    /// Stop pulling and end the subscription.
    public func stop() {
        pulls.cancel()
        let source = source
        Task { await Self.end(source) }
    }

    private static func end(_ source: Source) async {
        do {
            try await source.cancel()
        }
        catch {
            logger.warning(
                "a screen subscription did not end cleanly: \(String(reflecting: error))"
            )
        }
    }
}

extension PulledSubscription {
    /// The first value this subscription answers with, after which it ends:
    /// for a read that wants the value as it stands now, not its changes.
    public func firstValue() async throws -> Value {
        let value: Value
        do {
            value = try await next()
        }
        catch {
            do {
                try await cancel()
            }
            catch let ending {
                logger.warning(
                    "a first-value read did not end cleanly: \(String(reflecting: ending))"
                )
            }
            throw error
        }
        try await cancel()
        return value
    }
}

/// A subscription's values as a screen takes them: each value a bridge
/// object answers with, turned into the shape the screen reads. Ending it
/// ends the bridge object.
public struct PulledValues<Value: Sendable>: PulledSubscription {
    private let pull: @Sendable () async throws -> Value
    private let end: @Sendable () async throws -> Void

    private init(
        pull: @escaping @Sendable () async throws -> Value,
        end: @escaping @Sendable () async throws -> Void
    ) {
        self.pull = pull
        self.end = end
    }

    /// Each value of `source`, turned by `transform`.
    public init<Source: PulledSubscription>(
        _ source: Source,
        _ transform: @escaping @Sendable (Source.Value) -> Value
    ) {
        self.init(
            pull: { transform(try await source.next()) },
            end: { try await source.cancel() }
        )
    }

    /// Each value of `source`, as it answers.
    public init<Source: PulledSubscription>(_ source: Source)
    where Source.Value == Value {
        self.init(source) { $0 }
    }

    public func next() async throws -> Value {
        try await pull()
    }

    public func cancel() async throws {
        try await end()
    }

    #if DEBUG
        /// Values `read` answers with, for previews and tests; `cancel` is
        /// what ending them does.
        public init(
            read: @escaping @Sendable () async throws -> Value,
            cancel: @escaping @Sendable () async throws -> Void = {}
        ) {
            self.init(pull: read, end: cancel)
        }
    #endif
}

// Every bridge object a screen pulls from, read only through
// `ScreenSubscription`. The fast values' stream conforms in
// `LatestValues.swift`, the one file that touches it.
extension PlaybackValuesSubscription: PulledSubscription {}
extension PlaybackTimelineSubscription: PulledSubscription {}
extension QueueAdditionsSubscription: PulledSubscription {}
extension CastDevicesSubscription: PulledSubscription {}
extension ConfigSubscription: PulledSubscription {}
extension QueueSubscription: PulledSubscription {}
extension DownloadCountsSubscription: PulledSubscription {}
extension UploadsPausedSubscription: PulledSubscription {}
extension SyncStatusSubscription: PulledSubscription {}
extension SyncActivitySubscription: PulledSubscription {}
extension MembersSubscription: PulledSubscription {}
extension InvitesSubscription: PulledSubscription {}
extension AdminChangesSubscription: PulledSubscription {}
extension AddedDeviceSubscription: PulledSubscription {}
extension ProfilesSubscription: PulledSubscription {}
extension RequestedProfileSubscription: PulledSubscription {}
extension LibraryNameSubscription: PulledSubscription {}
extension SetupStatesSubscription: PulledSubscription {}
extension StorageSetupFinishingSubscription: PulledSubscription {}
extension AlbumBrowseSubscription: PulledSubscription {}
extension ArtistBrowseSubscription: PulledSubscription {}
extension ComposerBrowseSubscription: PulledSubscription {}
extension ComposerDetailRowsSubscription: PulledSubscription {}
extension AlbumSelectionSubscription: PulledSubscription {}
extension LibrarySearchSubscription: PulledSubscription {}
extension ArtistSearchSubscription: PulledSubscription {}
extension AlbumDetailSubscription: PulledSubscription {}
extension ReleaseDetailSubscription: PulledSubscription {}
extension ArtistDetailSubscription: PulledSubscription {}
extension ComposerDetailSubscription: PulledSubscription {}
extension WorkDetailSubscription: PulledSubscription {}
extension StorageBrowseSubscription: PulledSubscription {}
extension QueueUpcomingSubscription: PulledSubscription {}
#if os(macOS)
    extension ImportCountsSubscription: PulledSubscription {}
    extension CandidateRuntimeSubscription: PulledSubscription {}
    extension FolderScansSubscription: PulledSubscription {}
    extension LibraryStatusSubscription: PulledSubscription {}
    extension StorageInspectorSubscription: PulledSubscription {}
    extension ImportCandidateSubscription: PulledSubscription {}
    extension InvalidCandidateSubscription: PulledSubscription {}
    extension ReleaseFilesSubscription: PulledSubscription {}

    extension PulledValues where Value == BridgeImportListSnapshot {
        /// Each value of the import list's bridge object, which the list
        /// also moves and asks through `list`'s other calls.
        public init(_ list: any ImportListSubscriptionProtocol) {
            self.init(
                pull: { try await list.next() },
                end: { try await list.cancel() }
            )
        }
    }
#endif
