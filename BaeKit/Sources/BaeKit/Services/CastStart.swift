import Observation

/// Starting a cast to a receiver, shared by the cast picker and the Casting
/// settings page: the receiver it is connecting to while it does, the task, a
/// newer start cancelling an older one, and what happens when it ends.
@MainActor
@Observable
public final class CastStart {
    /// The receiver a cast is connecting to, while it does.
    public private(set) var connecting: BridgeCastReceiver?
    private var task: Task<Void, Never>?

    public init() {}

    /// Switch playback to `receiver`. Cancels the start an earlier call began.
    /// `onStarted` runs once playback is on the receiver; `onFailure` gets the
    /// reason it could not be. A start that was cancelled ends quietly.
    public func cast(
        to receiver: BridgeCastReceiver,
        with cast: Cast,
        onStarted: @escaping () -> Void,
        onFailure: @escaping @MainActor (any Error) -> Void
    ) {
        task?.cancel()
        connecting = receiver
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
                onFailure(error)
            }
        }
    }

    /// Stop waiting for a start still connecting: its place went away.
    public func cancel() {
        task?.cancel()
        task = nil
        connecting = nil
    }
}
