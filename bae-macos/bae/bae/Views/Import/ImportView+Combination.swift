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

    /// Read the release at `key` as the folders it is made of: a row's or the
    /// pane's "Keep as Separate Releases".
    func separateCandidate(_ key: String) {
        candidateMutationTasks[key]?.cancel()
        candidateMutationTasks[key] = Task {
            defer { candidateMutationTasks[key] = nil }
            await commitAndEndEditing()
            do {
                try await importer.separateCandidate(key)
            }
            catch is CancellationError {}
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// Read every release below the folder `key` names as one: a group
    /// header's "Combine as One Release". The release it makes is selected and
    /// revealed.
    func combineFolder(_ key: BridgeFolderReleaseDecisionKey) {
        let taskKey =
            "combine-folder:\(key.watchedFolderPath)/\(key.relativeFolderPath)"
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            await commitAndEndEditing()
            do {
                let release = try await importer.combineFolder(key)
                try Task.checkCancellation()
                uiStore.navigateToImportCandidate(release)
            }
            catch is CancellationError {}
            catch {
                uiStore.showError(error)
            }
        }
    }
}
