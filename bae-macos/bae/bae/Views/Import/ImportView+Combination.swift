import BaeKit
import SwiftUI

extension ImportView {
    func revealCandidateSources(_ key: Data) {
        let taskKey = ImportTaskKey.revealSources(key)
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            do {
                let paths = try await importer.candidateSourceFolders(key)
                try Task.checkCancellation()
                for path in paths { SystemActions.revealInFinder(folder: path) }
            }
            catch is CancellationError {}
            catch { uiStore.showError(error) }
        }
    }

    /// Read the release at `key` as the folders it is made of: a broken
    /// combined row's "Keep as Separate Releases".
    func separateCandidate(_ key: Data) {
        candidateMutationTasks[.candidate(key)]?.cancel()
        candidateMutationTasks[.candidate(key)] = Task {
            defer { candidateMutationTasks[.candidate(key)] = nil }
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

    /// Read the selected release as the folders it is made of: the pane's
    /// "Keep as Separate Releases". Core selects the releases it returns.
    func separateSelectedCandidate() {
        let taskKey = ImportTaskKey.separateSelection
        candidateMutationTasks[taskKey]?.cancel()
        candidateMutationTasks[taskKey] = Task {
            defer { candidateMutationTasks[taskKey] = nil }
            await commitAndEndEditing()
            do {
                try await importSelection.separate()
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
        let taskKey = ImportTaskKey.combineFolder(key)
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
