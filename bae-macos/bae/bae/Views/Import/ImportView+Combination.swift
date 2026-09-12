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

    func combineSelectedCandidates() {
        let taskKey = "combine-selected"
        let action = ImportCandidateCombineAction(
            importer: importer,
            uiStore: uiStore,
            listSlot: listSlot
        )
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            await commitAndEndEditing()
            await action.run()
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
