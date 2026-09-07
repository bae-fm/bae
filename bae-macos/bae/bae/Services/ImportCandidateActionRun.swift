import BaeKit
import Foundation
import Observation
import os.log

private let candidateActionLogger = Logger.bae("ImportCandidateActionRun")

struct ImportCandidateActionProgress {
    let action: BridgeCandidateAction
    let total: Int
    var completed: Int
}

/// A user-started batch survives selection changes. Each candidate commits
/// independently; failed and unattempted candidates remain selected.
@Observable
final class ImportCandidateActionRun {
    private(set) var progress: ImportCandidateActionProgress?
    private var task: Task<Void, Never>?

    var isRunning: Bool { task != nil || progress != nil }

    @MainActor
    @discardableResult
    func start(
        action: BridgeCandidateAction,
        candidates: [BridgeImportCandidateActionTarget],
        uiStore: UiStore,
        before: @escaping @MainActor () async -> Void,
        operation: @escaping @MainActor (String) async throws -> Void
    ) -> Task<Void, Never>? {
        guard !isRunning, !candidates.isEmpty else { return nil }
        task = Task {
            defer { task = nil }
            await before()
            guard !Task.isCancelled else { return }
            await perform(
                action: action,
                candidates: candidates,
                uiStore: uiStore,
                operation: operation
            )
        }
        return task
    }

    func cancel() { task?.cancel() }

    @MainActor
    func perform(
        action: BridgeCandidateAction,
        candidates: [BridgeImportCandidateActionTarget],
        uiStore: UiStore,
        operation: (String) async throws -> Void
    ) async {
        guard progress == nil, !candidates.isEmpty else { return }
        progress = ImportCandidateActionProgress(
            action: action,
            total: candidates.count,
            completed: 0
        )
        var successful: Set<String> = []
        var failures: [DisplayError] = []
        defer {
            switch action {
            case .importReady, .skip, .restore:
                uiStore.removeFolderCandidateSelection(successful)
            case .identify, .retryIdentification, .useFileMetadata,
                .clearMetadata:
                break
            }
            progress = nil
            if !failures.isEmpty {
                let details = failures.compactMap(\.detail)
                uiStore.showError(
                    DisplayError(
                        line: failures.map(\.line).joined(separator: "\n"),
                        detail: details.isEmpty
                            ? nil : details.joined(separator: "\n\n")
                    )
                )
            }
        }
        for candidate in candidates {
            if Task.isCancelled { return }
            do {
                try await operation(candidate.key)
                successful.insert(candidate.key)
            }
            catch is CancellationError { return }
            catch {
                candidateActionLogger.error(
                    "Candidate action \(String(describing: action)) failed for \(candidate.key): \(String(reflecting: error))"
                )
                if let error = DisplayError(error) {
                    failures.append(error.addingContext(candidate.displayName))
                }
            }
            progress?.completed += 1
        }
    }
}

/// Reads the dedicated core selection value. An action keeps only its targets.
@MainActor
struct ImportCandidateSelection {
    let importStore: ImportStore
    let uiStore: UiStore

    private var current: BridgeImportSelection? {
        guard let value = importStore.selection,
            Set(value.candidateKeys) == uiStore.selectedFolderCandidates
        else { return nil }
        return value
    }
    var canCombine: Bool { current?.canCombine == true }
    var offers: [BridgeImportCandidateActionOffer] { current?.offers ?? [] }
    func candidates(for action: BridgeCandidateAction)
        -> [BridgeImportCandidateActionTarget]
    {
        current?.offers.first(where: { $0.action == action })?.candidates ?? []
    }
}
