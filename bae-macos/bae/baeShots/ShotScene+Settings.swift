import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Settings panes over a library with a cloud home: the Library tab's
    /// sync section on a Dropbox shared folder, naming the folder coven
    /// recorded, with Open in browser.
    static let settingsScenes: [ShotScene] = [
        ShotScene(
            id: "settings-sync-dropbox",
            size: CGSize(width: 500, height: 640)
        ) {
            let reads = LibrarySettingsTabPreviewReads()
            return ShotStage(
                librarySettingsTabPreview(
                    configStore: PreviewData.connectedConfigStore(
                        storage: PreviewData.previewDropboxStorage
                    ),
                    reads: reads
                )
            ) { reads.answered }
        }
    ]
}
