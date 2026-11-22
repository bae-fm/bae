import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifyBarScenes: [ShotScene] = [
        identifyBarScene(
            "mac.component.identify-bar.import",
            row: .importing(commitControls())
        ),
        identifyBarScene(
            "mac.component.identify-bar.import-to-the-cloud",
            syncs: true,
            row: .importing(commitControls())
        ),
        identifyBarScene(
            "mac.component.identify-bar.retry",
            row: .importing(
                commitControls(
                    importStatus: .error(
                        error: .Diagnostic(
                            category: .import,
                            detail: "decode verification failed for 1 track"
                        )
                    )
                )
            )
        ),
        identifyBarScene("mac.component.identify-bar.nothing-to-import"),
        identifyBarScene(
            "mac.component.identify-bar.save",
            row: .saving(saveRow(changed: false))
        ),
        identifyBarScene(
            "mac.component.identify-bar.save-changed",
            row: .saving(saveRow(changed: true))
        ),
        identifyBarScene(
            "mac.component.identify-bar.save-reading",
            row: .saving(
                saveRow(
                    changed: true,
                    reading: "Reading Low Tide Hours (2019 rip).flac…"
                )
            )
        ),
    ]

    static func commitControls(
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
        row: ReleaseEditorActionRow = .importing(nil)
    ) -> ShotScene {
        component(
            id,
            width: 640,
            height: 50
        ) {
            importStores(
                identifyBar(row: row)
                    .frame(width: 640, alignment: .leading),
                syncs: syncs
            )
        }
    }

    /// The bar with the same action row as the editor that contains it.
    @ViewBuilder
    private static func identifyBar(
        row: ReleaseEditorActionRow
    ) -> some View {
        switch row {
        case .importing(let commit):
            bar {
                if let commit { ImportCommitRow(commit: commit) }
            }
        case .saving(let save):
            let controls = save.controls()
            bar { controls }.disabled(controls.isBusy)
        }
    }

    private static func bar<Content: View>(
        @ViewBuilder trailing: @escaping () -> Content
    ) -> some View {
        MacIdentifyBar(
            spacing: ThemeSpace.related,
            identifyDisabled: false,
            onAutomatic: {},
            onSearch: {},
            onMetadataCommand: { _ in },
            trailing: trailing
        )
    }
}
