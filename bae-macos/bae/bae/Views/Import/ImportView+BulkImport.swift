import BaeKit
import SwiftUI

extension ImportView {
    /// Run one action the selection or a row's menu offers — the pane a
    /// selection opens and the list's menu offer the same actions. One that
    /// replaces what a person may have chosen is asked about first.
    func requestCandidateAction(_ offer: ImportCandidateActionOffer) {
        guard offer.enabled else { return }
        if offer.action.needsConfirmation {
            candidateActionConfirmation = offer
            return
        }
        performCandidateAction(offer)
    }

    /// Core runs every action over the selection it holds, so a row's own
    /// action first makes that row the selection.
    func performCandidateAction(_ offer: ImportCandidateActionOffer) {
        if offer.action == .revealFolder, let key = offer.rowKey {
            revealCandidateSources(key)
            return
        }
        // One run over the selection at a time; the pane shows it running.
        guard !importSelection.isRunning else { return }
        let taskKey = "selection-action"
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            if let key = offer.rowKey {
                do { try await listSlot.selectOnly(key) }
                catch {
                    uiStore.showError(error)
                    return
                }
            }
            switch offer.action {
            case .combine:
                await commitAndEndEditing()
                await ImportCandidateCombineAction(
                    selection: importSelection,
                    uiStore: uiStore
                )
                .run()
            case .revealFolder:
                revealSelectionSources()
            case .import, .identify, .cancelIdentification, .cancelImport,
                .retryIdentification, .resetToFileMetadata, .clearMetadata,
                .separate, .skip, .restore:
                importSelection.start(
                    offer.action,
                    in: listSlot.view,
                    uiStore: uiStore,
                    before: commitAndEndEditing
                )
            }
        }
    }

    /// Show every selected folder in Finder.
    private func revealSelectionSources() {
        let taskKey = "reveal-selection"
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            do {
                let paths = try await importSelection.sourceFolders()
                try Task.checkCancellation()
                for path in paths { SystemActions.revealInFinder(path: path) }
            }
            catch is CancellationError {}
            catch { uiStore.showError(error) }
        }
    }
}
