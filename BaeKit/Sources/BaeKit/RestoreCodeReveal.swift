import Foundation
import Observation
import os.log

private let logger = Logger.bae("RestoreCodeReveal")

/// The person's restore code in Settings, from the screen opening to its
/// closing. The code is made when the screen opens and held masked, so Copy
/// and Print work at once; Show reveals it and Hide masks it again. Closing
/// the screen drops it, and cancels a generation still running, so a late
/// result never shows a code for a screen already closed.
@MainActor
@Observable
public final class RestoreCodeReveal {
    public enum State: Equatable {
        /// The screen isn't open: no code is held.
        case closed
        case loading
        /// The code, held for Copy and Print, shown only as its first groups.
        case masked(code: String)
        /// The code in groups of four, as core formats it.
        case shown(code: String)
        case failed(DisplayError)
    }

    public private(set) var state: State

    @ObservationIgnored
    private var task: Task<Void, Never>?

    #if DEBUG
        /// Every generation started, whether it was replaced, hidden or
        /// ran to its end, for a test to wait on.
        @ObservationIgnored
        private var started: [Task<Void, Never>] = []
    #endif

    public init() {
        state = .closed
    }

    #if DEBUG
        /// Starts in `state`, for previews that show a reveal already under
        /// way.
        public init(previewState: State) {
            state = previewState
        }
    #endif

    /// Make the code and hold it masked; a generation already running is
    /// replaced.
    public func open(
        _ generate:
            @escaping @Sendable () async throws -> BridgeRestoreCodeShare
    ) {
        task?.cancel()
        state = .loading
        let run = Task {
            do {
                // Cancelling this task also cancels the Rust call.
                let share = try await generate()
                try Task.checkCancellation()
                state = .masked(code: share.code)
            }
            catch is CancellationError {
                logger.debug("restore code generation cancelled")
            }
            catch {
                // A run this reveal replaced or closed owns no state any more,
                // whatever core reported for it.
                guard !Task.isCancelled else {
                    logger.debug("restore code generation cancelled")
                    return
                }
                logger.error(
                    "Failed to generate the restore code: \(String(reflecting: error))"
                )
                // A failure with no line is a cancellation from core; the
                // screen can open again.
                state = DisplayError(error).map(State.failed) ?? .closed
            }
        }
        task = run
        #if DEBUG
            started.append(run)
        #endif
    }

    #if DEBUG
        /// Return once every generation started so far has ended, replaced
        /// or not, so what a late one would have shown is already shown.
        public func generationsEnded() async {
            for run in started {
                await run.value
            }
        }
    #endif

    /// Show the held code.
    public func show() {
        if case .masked(let code) = state { state = .shown(code: code) }
    }

    /// Mask the shown code again.
    public func hide() {
        if case .shown(let code) = state { state = .masked(code: code) }
    }

    /// The screen closed: drop the code, cancelling a generation still
    /// running.
    public func close() {
        task?.cancel()
        task = nil
        state = .closed
    }
}
