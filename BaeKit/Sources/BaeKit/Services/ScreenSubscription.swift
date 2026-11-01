import Foundation
import os.log

private let logger = Logger.bae("ScreenSubscription")

/// A bridge object a screen pulls its values from: each `next()` answers
/// with the next value core delivers, and `cancel()` ends it.
public protocol PulledSubscription: AnyObject, Sendable {
    associatedtype Value: Sendable
    func next() async throws -> Value
    func cancel() async throws
}

/// The one way a screen reads a subscription: pull each value and hand it to
/// the screen on the main actor, until `stop()` or until the subscription
/// ends. A failed read is handed to `onError` and the pulls go on: the next
/// value answers the next read that succeeds.
@MainActor
public final class ScreenSubscription<Source: PulledSubscription> {
    private let source: Source
    private var pulls: Task<Void, Never>?

    public init(
        _ source: Source,
        onValue: @escaping @MainActor (Source.Value) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        self.source = source
        pulls = Task { @MainActor in
            while !Task.isCancelled {
                do {
                    onValue(try await source.next())
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch is CancellationError {
                    return
                }
                catch {
                    onError(error)
                }
            }
        }
    }

    deinit {
        pulls?.cancel()
        let source = source
        Task { await Self.end(source) }
    }

    /// Stop pulling and end the subscription.
    public func stop() {
        pulls?.cancel()
        pulls = nil
        let source = source
        Task { await Self.end(source) }
    }

    private nonisolated static func end(_ source: Source) async {
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

// Every bridge object a screen pulls from, read only through
// `ScreenSubscription`.
extension PlaybackValuesSubscription: PulledSubscription {}
extension PlaybackTimelineSubscription: PulledSubscription {}
extension QueueAdditionsSubscription: PulledSubscription {}
#if os(macOS)
    extension ImportCountsSubscription: PulledSubscription {}
    extension CandidateRuntimeSubscription: PulledSubscription {}
#endif
