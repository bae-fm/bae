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
    start: (BridgeReleaseUserEdit) async throws -> Void
) async {
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
        try await start(userEdit)
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
        guard case .folder = candidate.source else {
            return
        }
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        // Only a pick the user made crosses. With none, bae-core lands the
        // picked release's own first cover option rather than importing bare —
        // sending the pane's shown default back would make it indistinguishable
        // from a choice.
        let coverSelection = candidate.coverPick?.selection

        let storageMode = configStore.config.importStorageMode(
            managed: storageManaged
        )

        // The edit is the album fields over the mapping table's rows, and it
        // exists exactly when an identity has been settled — a pick's claim, or
        // `.unknown`. The commit bar is the only surface carrying this button
        // and it renders on the same condition, so absence here is a structural
        // bug.
        guard let identityChoice = candidate.identityChoice,
            let commitEdit = candidate.commitEdit
        else {
            fatalError("commit reached with nothing settled to commit")
        }

        Task {
            await commitImport(
                store: importStore,
                key: candidate.key,
                rawEdit: commitEdit
            ) {
                try await importer.startImport(
                    ImportCommitRequest(
                        candidateKey: candidate.key,
                        selectedCover: coverSelection,
                        storageMode: storageMode,
                        pin: storagePinned,
                        identityChoice: identityChoice,
                        userEdit: $0
                    )
                )
            }
        }
    }
}
