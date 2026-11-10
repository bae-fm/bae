import Observation

/// The cast the person started last, while it connects. There is one for the
/// app, not one per place that lists receivers: the picker and the Casting
/// settings page show the same start, and closing the one that began it leaves
/// it connecting. A newer start calls off the one before it. A start that
/// fails says why through `report`, wherever the person is by then.
@MainActor
@Observable
public final class CastStart {
    /// The receiver a cast is connecting to, while it does.
    public private(set) var connecting: BridgeCastReceiver?
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

    /// Switch playback to `receiver`, calling off the start an earlier call
    /// began. `onStarted` runs once playback is on the receiver.
    public func cast(
        to receiver: BridgeCastReceiver,
        onStarted: @escaping @MainActor () -> Void
    ) {
        task?.cancel()
        connecting = receiver
        let cast = cast
        task = Task {
            do {
                try await cast.castTo(receiver.id)
                guard !Task.isCancelled else { return }
                connecting = nil
                onStarted()
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
    }

    #if DEBUG
        /// A start over `Cast.stub()`, whose casts start at once.
        public static func stub() -> CastStart {
            CastStart(cast: .stub(), report: { _ in })
        }
    #endif
}
