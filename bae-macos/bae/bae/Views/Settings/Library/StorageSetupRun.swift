import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("StorageSetupRun")

/// A sheet's storage setup while it runs, and the person's Cancel of it.
/// Before its point of no return a cancel undoes the setup, which takes
/// effect between its steps rather than in the middle of a request, so the
/// sheet shows the cancel under way until the setup's call ends, then
/// `onCancelled` closes it. Past that point the setup finishes, and Cancel
/// isn't offered.
@MainActor
@Observable
final class StorageSetupRun {
    enum State {
        case idle
        /// A setup runs.
        case running(StorageSetupFollowing)
        /// The person cancelled the running setup; its call ends once coven
        /// has undone what it wrote.
        case cancelling
    }

    private(set) var state: State = .idle
    @ObservationIgnored
    private var task: Task<Void, Never>?
    @ObservationIgnored
    private let onCancelled: () -> Void

    init(onCancelled: @escaping () -> Void) {
        self.onCancelled = onCancelled
    }

    var isRunning: Bool {
        if case .idle = state {
            false
        }
        else {
            true
        }
    }

    /// Whether the person cancelled the running setup, which coven is undoing.
    var isCancelling: Bool {
        if case .cancelling = state {
            true
        }
        else {
            false
        }
    }

    /// Whether Cancel can still stop the running setup: until it is past
    /// its point of no return, and not once the person has cancelled it.
    var cancelOffered: Bool {
        guard case .running(let following) = state else { return false }
        return !following.finishing
    }

    /// Run `operation` on `following`'s setup: the setup it ends in goes to
    /// `onConnected`, and a failure to `onFailed`, unless the person
    /// cancelled it.
    func start(
        _ following: StorageSetupFollowing,
        operation:
            @escaping (BridgeLibrarySetup) async throws -> BridgeStorageSetup,
        onConnected: @escaping (BridgeStorageSetup) -> Void,
        onFailed: @escaping (any Error) -> Void
    ) {
        state = .running(following)
        task = Task {
            defer { task = nil }
            let failure: any Error
            do {
                let setup = try await operation(following.setup)
                state = .idle
                onConnected(setup)
                return
            }
            catch {
                failure = error
            }
            if case .cancelling = state {
                state = .idle
                logger.debug(
                    "Storage setup cancelled: \(String(reflecting: failure))"
                )
                onCancelled()
                return
            }
            state = .idle
            if failure is CancellationError {
                logger.debug("Storage setup's call dropped")
                return
            }
            logger.error("Storage setup failed: \(String(reflecting: failure))")
            onFailed(failure)
        }
    }

    /// The person pressed Cancel while the setup runs. Before its point of
    /// no return coven undoes it and its call ends cancelled. A setup coven
    /// isn't running yet is dropped with its call, which cancels it. Past the
    /// point the setup finishes, and Cancel goes as it says so.
    func cancel(_ sync: Sync) {
        guard case .running = state else { return }
        switch sync.cancelStorageSetup() {
        case .undoing:
            state = .cancelling
        case .noSetup:
            state = .cancelling
            task?.cancel()
        case .finishing:
            logger.info("Cancel came past setup's point of no return")
        }
    }

    /// The sheet went away: dropping the setup's call cancels its setup.
    func leave() {
        task?.cancel()
    }
}

/// What the running setup shows above its sheet's buttons: the step it runs
/// over its bar, or the person's cancel under way. Where the setup checks a
/// folder the person chose, the check says so.
struct StorageSetupRunLine: View {
    let run: StorageSetupRun
    var checkedFolder: String?

    var body: some View {
        switch run.state {
        case .idle:
            EmptyView()
        case .running(let following):
            MacProgressContainer(
                steps: following.steps,
                titled: { progress in
                    if let checkedFolder, case .checkingLocation = progress.step
                    {
                        String(
                            localized:
                                "Checking the folder \(checkedFolder)…",
                            comment:
                                "Dropbox or OneDrive setup, while bae checks the folder the person chose holds nothing else; the argument is the name of that folder."
                        )
                    }
                    else {
                        progress.title
                    }
                },
                idle: { EmptyView() }
            )
        case .cancelling:
            MacSpinner("Cancelling…")
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
