import BaeKit
import SwiftUI

/// The card's commit controls: the folder check, how many rows nobody has
/// named, storage, and Import, which nothing here disables.
struct ImportCommitControls {
    let untitledTrackCount: Int
    /// Why the folder keeps this candidate out of a bulk import, such as a
    /// release that lists no tracks; shown beside Import, which stays
    /// available.
    let folderCheck: BridgeFolderCheck?
    /// What the last import left: a failure, which Import retries, or
    /// nothing.
    let importStatus: BridgeCandidateImportStatus?
    let importDestination: Binding<BridgeImportDestination>
    let actions: ImportCommitActions
}

/// The identify bar's trailing controls on an import's card: how many rows
/// nobody has named, where the import goes when the library syncs, and Import.
struct ImportCommitRow: View {
    let commit: ImportCommitControls

    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            if commit.untitledTrackCount > 0 {
                Text(
                    localizedCoreString(
                        "ui.import.commit.unanswered",
                        commit.untitledTrackCount
                    )
                )
                .themeText(.detail)
                .foregroundStyle(Theme.warning)
            }
            if configStore.syncs {
                MacImportDestination(
                    destination: commit.importDestination
                )
            }
            ImportConfirmationCardAction(
                importStatus: commit.importStatus,
                onConfirmImport: commit.actions.confirmImport
            )
        }
    }
}
