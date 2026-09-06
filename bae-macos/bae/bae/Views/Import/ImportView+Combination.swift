import BaeKit
import SwiftUI

extension ImportView {
    func revealCandidateSources(_ key: String) {
        let taskKey = "reveal:\(key)"
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            do {
                let paths = try await importer.candidateSourceFolders(key)
                try Task.checkCancellation()
                for path in paths { SystemActions.revealInFinder(path: path) }
            }
            catch is CancellationError {}
            catch { uiStore.showError(error) }
        }
    }

    func reviewSelectedCombination() {
        let keys = uiStore.selectedFolderCandidates.sorted()
        candidateMutationTasks["combination-review"]?.cancel()
        candidateMutationTasks["combination-review"] = Task {
            defer { candidateMutationTasks["combination-review"] = nil }
            await commitAndEndEditing()
            do {
                let review = try await importer.reviewCombination(keys)
                try Task.checkCancellation()
                let state = try ImportCombinationReview(review: review)
                uiStore.presentModal {
                    ImportCombinationReviewView(
                        review: state,
                        onCancel: { uiStore.dismissModal() },
                        onCombined: { key in
                            uiStore.dismissModal()
                            uiStore.setFolderCandidateSelection([key])
                            listSlot.requestCandidateReveal(key)
                        }
                    )
                }
            }
            catch is CancellationError {}
            catch {
                uiStore.showError(error)
            }
        }
    }

    func separateCombination(_ key: String) {
        candidateMutationTasks[key]?.cancel()
        candidateMutationTasks[key] = Task {
            defer { candidateMutationTasks[key] = nil }
            await commitAndEndEditing()
            do {
                try await importer.separateCombination(key)
                uiStore.removeFolderCandidateSelection([key])
            }
            catch is CancellationError {}
            catch {
                uiStore.showError(error)
            }
        }
    }
}
