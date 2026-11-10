import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Files section of a release, as a Found folder draws it (its track
    /// sheet still bindable) and as an imported release's pane draws it, and
    /// the release editor the album's Edit Release sheet shows, its Files
    /// under the tracks.
    static let releaseFilesScenes: [ShotScene] = [
        ShotScene(
            id: "release-files-found",
            size: CGSize(width: 560, height: 300)
        ) {
            ShotStage(
                ReleaseFilesView(
                    rows: PreviewData.releaseFilesRows,
                    onOpenDocument: { _, _ in },
                    sheetActions: inertMappingActions()
                )
                .padding(ThemeSpace.section)
                .background(Theme.background)
            )
        },
        ShotScene(
            id: "release-files-imported",
            size: CGSize(width: 560, height: 300)
        ) {
            ShotStage(
                ReleaseFilesView(
                    rows: PreviewData.releaseFilesRows,
                    onOpenDocument: { _, _ in }
                )
                .padding(ThemeSpace.section)
                .background(Theme.background)
            )
        },
        ShotScene(
            id: "release-files-edit",
            size: CGSize(width: 1_100, height: 1_100)
        ) {
            let library = PreviewReleaseLibrary(
                draft: PreviewData.releaseEditDraft(trackCount: 3)
            )
            // The editor the sheet scrolls, whole: the sheet holds it at
            // most 860 points tall.
            return ShotStage(
                ReleaseEditorView(
                    session: ReleaseMetadataEditSession(
                        releaseId: "release-preview",
                        opened: library.opened(),
                        commands: library.commands()
                    ),
                    displayName: "Album Title \u{00B7} 2019 \u{00B7} CD",
                    onViewAlbum: { _ in },
                    onSaved: { _ in }
                )
                .padding(ThemeSpace.section)
                .background(Theme.background)
                .environment(PreviewData.artistAssignmentsLibrary())
                .environment(SettingsNavigation())
                .albumDetailPreviewEnvironment()
                .candidateReaderPreviewEnvironment()
                .mainWindowPreviewEnvironment()
            )
        },
    ]
}
