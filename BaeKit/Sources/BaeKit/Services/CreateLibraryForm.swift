import Foundation
import Observation
import os.log

private let logger = Logger.bae("CreateLibraryForm")

/// The step that creates a library: the creator's name, which becomes their
/// profile, and the running create. The apps' step renders it and calls its
/// actions.
@MainActor
@Observable
public final class CreateLibraryForm {
    public var name = ""
    public private(set) var isRunning = false
    /// The running create's steps, nil while none runs.
    public private(set) var steps: LibrarySetupSteps?
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private var task: Task<Void, Never>?
    @ObservationIgnored
    private let createLibrary:
        @Sendable (_ creatorName: String, BridgeLibrarySetup) async throws ->
            OpenedLibrary
    @ObservationIgnored
    private let makeSteps: @Sendable () -> LibrarySetupSteps
    @ObservationIgnored
    private let onLibraryReady: @MainActor (OpenedLibrary) -> Void

    init(
        createLibrary:
            @escaping @Sendable (_ creatorName: String, BridgeLibrarySetup)
            async throws -> OpenedLibrary,
        makeSteps: @escaping @Sendable () -> LibrarySetupSteps,
        onLibraryReady: @escaping @MainActor (OpenedLibrary) -> Void
    ) {
        self.createLibrary = createLibrary
        self.makeSteps = makeSteps
        self.onLibraryReady = onLibraryReady
    }

    /// The name as it is sent: without the spaces around it.
    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Whether Create can run: a name is typed and no create is running.
    public var canCreate: Bool { !trimmedName.isEmpty && !isRunning }

    /// Create the library and the creator's profile under the typed name.
    public func create() {
        guard canCreate else { return }
        let creatorName = trimmedName
        let create = createLibrary
        let steps = makeSteps()
        self.steps = steps
        isRunning = true
        error = nil
        task = Task {
            defer {
                isRunning = false
                self.steps = nil
            }
            do {
                let library = try await create(creatorName, steps.setup)
                onLibraryReady(library)
            }
            catch is CancellationError {}
            catch {
                logger.error(
                    "Library creation failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }

    /// Stop a running create, as the step is left.
    public func cancel() {
        task?.cancel()
    }
}
