import BaeKit
import SwiftUI

/// Commit tail for a confirmed import. Shapes the candidate's raw editor
/// form into the wire edit — writing bae-core's `.invalid` reason onto the
/// candidate and bailing if it doesn't validate, which the commit bar states as
/// a banner rather than pre-empting with a disabled button — then runs `start`,
/// writing any thrown error onto the candidate.
@MainActor
private func commitImport(
    store: ImportStore,
    key: String,
    rawEdit: BridgeRawReleaseEdit,
    start: (BridgeReleaseUserEdit) throws -> Void
) {
    let userEdit: BridgeReleaseUserEdit
    switch shapeReleaseEdit(raw: rawEdit) {
    case .valid(let edit):
        userEdit = edit
    case .invalid(let reason):
        store.mutateCandidate(forKey: key) {
            $0.error = reason.localizedMessage
        }
        return
    }
    do {
        try start(userEdit)
    }
    catch {
        store.mutateCandidate(forKey: key) {
            $0.error = error.displayLine
        }
    }
}

// MARK: - Commit

extension ImportView {
    func commitConfirmedImport(candidate: Candidate) {
        guard case .folder(let folderPath, _) = candidate.source else {
            return
        }
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        let coverSelection = candidate.selectedCover?.selection

        let storageMode = configStore.config.importStorageMode(
            managed: storageManaged
        )

        // The identity choice was picked at row-time (or set to
        // `.unknown` by the "Add as Unknown" link) and stashed on the
        // candidate; the editor overlay is the candidate's current
        // `editValues` (seeded from the prefetch or the file-tag projection,
        // possibly mutated by the user on the mapping pane). Both fields are
        // written before the commit bar appears, which is the only surface
        // carrying this button — absence here is a structural bug.
        guard let identityChoice = candidate.identityChoice,
            let editValues = candidate.editValues
        else {
            fatalError("commit reached without identity choice or edit values")
        }

        commitImport(
            store: importStore,
            key: candidate.key,
            rawEdit: editValues
        ) {
            try importer.startImport(
                candidate.key,
                folderPath,
                coverSelection,
                storageMode,
                storagePinned,
                identityChoice,
                $0
            )
        }
    }
}
