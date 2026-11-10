import Observation

/// The cast the person started last, while it connects. There is one for the
/// app, not one per place that lists receivers: the picker and the Casting
/// settings page show the same start, and closing the one that began it leaves
/// it connecting. A newer start calls off the one before it. A start that
/// fails says why through `report`, wherever the person is by then. A start
/// that connects shows as `started`; a place that began one closes itself when
/// `started` is the `Attempt` it was given, so a place opened later never
/// closes for a start it did not ask for.
@MainActor
@Observable
public final class CastStart {
    /// The receiver a cast is connecting to, while it does.
    public private(set) var connecting: BridgeCastReceiver?
    /// The latest start that reached playback on its receiver.
    public private(set) var started: Attempt?
    private var attempts = 0
    private let cast: Cast
    private let report: @MainActor (any Error) -> Void
    private var task: Task<Void, Never>?

    /// Starts go through `cast`. Why one failed goes to `report`, the app's
    /// place for errors, which shows nothing for an error core gives no line:
    /// a cast called off by stopping casting, turning it off, or another cast.
    public init(cast: Cast, report: @escaping @MainActor (any Error) -> Void) {
        self.cast = cast
        self.report = report
    }

    /// One call to `cast(to:)`. `started` equals it once that start connects.
    public struct Attempt: Equatable, Sendable {
        fileprivate let number: Int
    }

    /// Whether `attempt`, what a place was given when it asked, is the start
    /// that connected. A place that asked nothing, or asked an earlier start
    /// that a later one replaced, gets false.
    public func didStart(_ attempt: Attempt?) -> Bool {
        attempt != nil && attempt == started
    }

    /// Switch playback to `receiver`, calling off the start an earlier call
    /// began. Returns the attempt `started` becomes once playback is on the
    /// receiver.
    @discardableResult
    public func cast(to receiver: BridgeCastReceiver) -> Attempt {
        task?.cancel()
        connecting = receiver
        attempts += 1
        let attempt = Attempt(number: attempts)
        let cast = cast
        task = Task {
            do {
                try await cast.castTo(receiver.id)
                guard !Task.isCancelled else { return }
                connecting = nil
                started = attempt
            }
            catch is CancellationError {
                return
            }
            catch {
                // A newer start owns `connecting` now.
                guard !Task.isCancelled else { return }
                connecting = nil
                report(error)
            }
        }
        return attempt
    }

    #if DEBUG
        /// A start over `Cast.stub()`, whose casts start at once.
        public static func stub() -> CastStart {
            CastStart(cast: .stub(), report: { _ in })
        }
    #endif
}
