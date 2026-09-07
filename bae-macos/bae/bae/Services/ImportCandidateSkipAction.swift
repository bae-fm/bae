import BaeKit
import Foundation

/// The single bulk-skip operation used by the mapping pane and Command-E.
/// Eligibility is read from the current core projection at invocation time;
/// completion removes only successful candidates, preserving failed requests
/// and rows selected while its calls were in flight.
@MainActor
struct ImportCandidateSkipAction {
    let importer: Importer
    let importStore: ImportStore
    let uiStore: UiStore

    var label: String {
        BridgeCandidateAction.skip.label(count: eligibleCandidates.count)
    }

    var isEnabled: Bool {
        !uiStore.candidateActionRun.isRunning && !eligibleCandidates.isEmpty
    }

    @discardableResult
    func start() -> Task<Void, Never>? {
        uiStore.candidateActionRun.start(
            action: .skip,
            candidates: eligibleCandidates,
            uiStore: uiStore,
            before: {},
            operation: { key in
                try await importer.setCandidateSkipped(key, true)
            }
        )
    }

    /// The dedicated selected-key query's targets for the Skip command.
    private var eligibleCandidates: [BridgeImportCandidateActionTarget] {
        ImportCandidateSelection(importStore: importStore, uiStore: uiStore)
            .candidates(for: .skip)
    }
}
