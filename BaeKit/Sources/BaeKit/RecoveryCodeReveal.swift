import Foundation
import Observation
import os.log

private let logger = Logger.bae("RecoveryCodeReveal")

/// Whether the person's recovery code shows in Settings, shared by the macOS
/// and iOS settings screens. The code stays hidden until the person asks for
/// it, is generated on that request, and is hidden again on request or when
/// the screen goes away. Hiding cancels a generation still running, so a late
/// result never shows a code the person already put away.
@MainActor
@Observable
public final class RecoveryCodeReveal {
    public enum State: Equatable {
        case hidden
        case loading
        /// The code exactly as coven formats it.
        case shown(code: String)
        case failed(DisplayError)
    }

    public private(set) var state: State

    @ObservationIgnored
    private var task: Task<Void, Never>?

    public init() {
        state = .hidden
    }

    #if DEBUG
        /// Starts in `state`, for previews that show a reveal already under
        /// way.
        public init(previewState: State) {
            state = previewState
        }
    #endif

    /// Generate the code and show it; a generation already running is
    /// replaced.
    public func show(
        _ generate:
            @escaping @Sendable () async throws -> BridgeRestoreCodeShare
    ) {
        task?.cancel()
        state = .loading
        task = Task {
            do {
                // Cancelling this task also cancels the Rust call.
                let share = try await generate()
                try Task.checkCancellation()
                state = .shown(code: share.code)
            }
            catch is CancellationError {
                logger.debug("recovery code generation cancelled")
            }
            catch {
                // A run this reveal replaced or hid owns no state any more,
                // whatever core reported for it.
                guard !Task.isCancelled else {
                    logger.debug("recovery code generation cancelled")
                    return
                }
                logger.error(
                    "Failed to generate the recovery code: \(error.localizedDescription)"
                )
                // A failure with no line is a cancellation from core; the
                // person can ask again.
                state = DisplayError(error).map(State.failed) ?? .hidden
            }
        }
    }

    /// Hide the code, cancelling a generation still running.
    public func hide() {
        task?.cancel()
        task = nil
        state = .hidden
    }
}
