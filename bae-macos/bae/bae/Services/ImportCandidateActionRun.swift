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
        targets: [ImportCandidateActionTarget],
        uiStore: UiStore,
        before: @escaping @MainActor () async -> Void,
        operation: @escaping @MainActor (String) async throws -> Void
    ) -> Task<Void, Never>? {
        guard !isRunning, !targets.isEmpty else { return nil }
        task = Task {
            defer { task = nil }
            await before()
            guard !Task.isCancelled else { return }
            await perform(
                action: action,
                targets: targets,
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
        targets: [ImportCandidateActionTarget],
        uiStore: UiStore,
        operation: (String) async throws -> Void
    ) async {
        guard progress == nil, !targets.isEmpty else { return }
        progress = ImportCandidateActionProgress(
            action: action,
            total: targets.count,
            completed: 0
        )
        var successful: Set<String> = []
        var failures: [DisplayError] = []
        defer {
            switch action {
            // The candidate is gone from where it was selected: into the
            // library, off the queue, or read as other releases.
            case .importReady, .skip, .restore, .separate:
                uiStore.removeFolderCandidateSelection(successful)
            case .identify, .cancelIdentification, .retryIdentification,
                .resetToFileMetadata, .clearMetadata, .cancelImport, .combine,
                .revealFolder:
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
        for target in targets {
            if Task.isCancelled { return }
            do {
                try await operation(target.key)
                successful.insert(target.key)
            }
            catch is CancellationError { return }
            catch {
                candidateActionLogger.error(
                    "Candidate action \(String(describing: action)) failed for \(target.key): \(String(reflecting: error))"
                )
                if let error = DisplayError(error) {
                    failures.append(error.addingContext(target.displayName))
                }
            }
            progress?.completed += 1
        }
    }
}

/// One candidate an action runs on: its key, and the name a report of the
/// run gives it.
struct ImportCandidateActionTarget: Hashable {
    let key: String
    let displayName: String
}

/// One action a selection offers, over the candidates it applies to, and
/// whether it can run as the selection stands — core's answer, for the pane a
/// multi-selection opens and the list's menu alike.
struct ImportCandidateActionOffer: Identifiable {
    let action: BridgeCandidateAction
    let targets: [ImportCandidateActionTarget]
    let enabled: Bool
    var id: BridgeCandidateAction { action }
    var keys: [String] { targets.map(\.key) }

    /// What `members` — each candidate with the actions its live state offers
    /// — can be told to do together, as core decides it.
    static func offers(
        for members: [(
            target: ImportCandidateActionTarget,
            actions: [BridgeCandidateAction]
        )]
    ) -> [ImportCandidateActionOffer] {
        let targets = Dictionary(
            members.map { ($0.target.key, $0.target) },
            uniquingKeysWith: { first, _ in first }
        )
        return bridgeCandidateSelectionOffers(
            members: members.map {
                BridgeSelectionMember(
                    candidateKey: $0.target.key,
                    actions: $0.actions
                )
            }
        )
        .map { offer in
            ImportCandidateActionOffer(
                action: offer.action,
                targets: offer.candidateKeys.compactMap { targets[$0] },
                enabled: offer.enabled
            )
        }
    }
}

/// What the UI's selection can be told to do: every selected candidate's live
/// actions, joined by core into the selection's offers.
@MainActor
struct ImportCandidateSelection {
    let importStore: ImportStore
    let uiStore: UiStore

    var offers: [ImportCandidateActionOffer] {
        ImportCandidateActionOffer.offers(
            for: uiStore.selectedFolderCandidates.sorted()
                .compactMap { key in
                    guard let candidate = importStore.selectedCandidates[key]
                    else { return nil }
                    return (
                        ImportCandidateActionTarget(
                            key: key,
                            displayName: candidate.displayName
                        ),
                        candidate.live?.actions ?? []
                    )
                }
        )
    }

    /// The selected candidates `action` runs on, as the selection offers it.
    func candidates(
        for action: BridgeCandidateAction
    ) -> [ImportCandidateActionTarget] {
        offers.first { $0.action == action }?.targets ?? []
    }
}
