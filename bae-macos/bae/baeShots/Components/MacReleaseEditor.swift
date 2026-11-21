import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleaseEditor`, one shot for each row of its preview. The editor
    /// is drawn in the two places the app shows it: the import pane's
    /// release editor and the library's Edit release sheet, so each row
    /// is the board scene of that place, shot again under the row's name.
    static let macReleaseEditorScenes: [ShotScene] = [
        reshot(
            "mac.component.release-editor.import",
            from: .identified
        ),
        reshot(
            "mac.component.release-editor.two-labels",
            from: .identified
        ),
        reshot(
            "mac.component.release-editor.import-to-the-cloud",
            from: .identified
        ),
        reshot(
            "mac.component.release-editor.import-with-a-folder-check",
            from: .identified
        ),
        reshot(
            "mac.component.release-editor.retry-import",
            from: .importing
        ),
        reshot(
            "mac.component.release-editor.nothing-to-import",
            from: .nothingRead
        ),
        reshot(
            "mac.component.release-editor.import-reading-a-file",
            from: .importing
        ),
        reshot(
            "mac.component.release-editor.import-album-picker-open",
            from: .identified
        ),
        reshot(
            "mac.component.release-editor.import-nothing-read",
            from: .nothingRead
        ),
        reshot(
            "mac.component.release-editor.save",
            from: .edit
        ),
        reshot(
            "mac.component.release-editor.save-changed",
            from: .edit
        ),
        reshot(
            "mac.component.release-editor.save-reading-a-file",
            from: .reading
        ),
        reshot(
            "mac.component.release-editor.save-album-picker-open",
            from: .albumPicker
        ),
    ]

    /// The board scenes the rows are shot from.
    @MainActor
    private enum Source {
        case identified, importing, nothingRead, edit, reading, albumPicker

        /// The import pane's scenes, and the editor sheet's, in the order
        /// their lists declare them.
        var scene: ShotScene {
            switch self {
            case .identified: ShotScene.importPaneScenes[1]
            case .importing: ShotScene.importPaneScenes[2]
            case .nothingRead: ShotScene.importPaneScenes[4]
            case .edit: ShotScene.albumEditorScenes[0]
            case .albumPicker: ShotScene.albumEditorScenes[2]
            case .reading: ShotScene.albumEditorScenes[4]
            }
        }
    }

    /// The scene `source` draws, under the name `id`.
    private static func reshot(_ id: String, from source: Source) -> ShotScene {
        let scene = source.scene
        return ShotScene(
            id: id,
            size: scene.size,
            frame: scene.frame,
            stage: scene.stage
        )
    }
}
