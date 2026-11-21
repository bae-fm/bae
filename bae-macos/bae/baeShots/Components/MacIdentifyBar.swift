import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifyBarScenes: [ShotScene] = [
        identifyBarScene(
            "mac.component.identify-bar.import",
            commit: commitControls()
        ),
        identifyBarScene(
            "mac.component.identify-bar.import-to-the-cloud",
            syncs: true,
            commit: commitControls()
        ),
        identifyBarScene(
            "mac.component.identify-bar.retry",
            commit: commitControls(
                importStatus: .error(
                    error: .Diagnostic(
                        category: .import,
                        detail: "decode verification failed for 1 track"
                    )
                )
            )
        ),
        identifyBarScene("mac.component.identify-bar.nothing-to-import"),
        identifyBarScene("mac.component.identify-bar.save"),
        identifyBarScene("mac.component.identify-bar.save-changed"),
        identifyBarScene("mac.component.identify-bar.save-reading"),
    ]

    private static func commitControls(
        folderCheck: BridgeFolderCheck? = nil,
        importStatus: BridgeCandidateImportStatus? = nil
    ) -> ImportCommitControls {
        ImportCommitControls(
            untitledTrackCount: 0,
            folderCheck: folderCheck,
            importStatus: importStatus,
            importDestination: .constant(.cloud),
            actions: ImportCommitActions(
                confirmImport: {},
                viewInLibrary: { _ in }
            )
        )
    }

    private static func identifyBarScene(
        _ id: String,
        syncs: Bool = false,
        commit: ImportCommitControls? = nil
    ) -> ShotScene {
        component(
            id,
            width: 640,
            height: 50
        ) {
            importStores(
                identifyBar(commit: commit)
                    .frame(width: 640, alignment: .leading),
                syncs: syncs
            )
        }
    }

    /// The bar over an import's `commit` controls.
    private static func identifyBar(
        commit: ImportCommitControls?
    ) -> some View {
        MacIdentifyBar(
            spacing: ThemeSpace.related,
            identifyDisabled: false,
            onAutomatic: {},
            onSearch: {},
            onMetadataCommand: { _ in },
            trailing: {
                if let commit {
                    ImportCommitRow(commit: commit)
                }
            }
        )
    }
}
