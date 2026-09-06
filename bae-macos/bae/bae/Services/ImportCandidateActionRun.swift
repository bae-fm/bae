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
        candidates: [Candidate],
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
        candidates: [Candidate],
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

/// Intersects the UI's selection with the commands each current row offers.
@MainActor
struct ImportCandidateSelection {
    let importStore: ImportStore
    let uiStore: UiStore

    var canCombine: Bool {
        let keys = uiStore.selectedFolderCandidates
        return keys.count >= 2
            && keys.allSatisfy {
                importStore.selectedCandidates[$0]?.detail?.candidate
                    .compositionAction == .combine
            }
    }

    var offers: [ImportCandidateActionOffer] {
        let actions: [BridgeCandidateAction] = [
            .importReady, .identify, .retryIdentification,
            .useFileMetadata, .clearMetadata, .skip, .restore,
        ]
        return actions.compactMap { action in
            let eligible = candidates(for: action)
            return eligible.isEmpty
                ? nil
                : ImportCandidateActionOffer(
                    action: action,
                    candidates: eligible
                )
        }
    }

    func candidates(for action: BridgeCandidateAction) -> [Candidate] {
        uiStore.selectedFolderCandidates.sorted()
            .compactMap { key in
                guard let candidate = importStore.selectedCandidates[key],
                    candidate.row?.actions.contains(action) == true
                else { return nil }
                return candidate
            }
    }
}

struct ImportCandidateActionOffer: Identifiable {
    let action: BridgeCandidateAction
    let candidates: [Candidate]
    var id: BridgeCandidateAction { action }
}
