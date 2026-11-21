import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportFolderLine`, one shot for each row of its preview.
    static let macImportFolderLineScenes: [ShotScene] = [
        component(
            "mac.component.import-folder-line.default",
            page: true,
            height: 50
        ) {
            MacImportFolderLine(
                tab: .pending,
                folderName: "Mara Quill - Low Tide Hours (1992) [FLAC]",
                folderPaths: [],
                onNavigateToPlacement: {}
            )
            .frame(width: 380, alignment: .leading)
        }
    ]
}
