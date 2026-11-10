import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    #if BAE_OAUTH_PROVIDERS
        /// Set up sync's folder picker for Dropbox and OneDrive, each with
        /// its list, the name field for a new folder, the folder it made and
        /// a folder setup found holding other files.
        static let folderPickerScenes: [ShotScene] = [
            folderPicker("folder-picker-dropbox", .dropbox, .list),
            folderPicker("folder-picker-dropbox-naming", .dropbox, .naming),
            folderPicker("folder-picker-dropbox-made", .dropbox, .made),
            folderPicker(
                "folder-picker-dropbox-not-empty",
                .dropbox,
                .notEmpty
            ),
            folderPicker("folder-picker-dropbox-none", .dropbox, .none),
            folderPicker("folder-picker-onedrive", .oneDrive, .list),
            folderPicker("folder-picker-onedrive-naming", .oneDrive, .naming),
            folderPicker("folder-picker-onedrive-made", .oneDrive, .made),
            folderPicker(
                "folder-picker-onedrive-not-empty",
                .oneDrive,
                .notEmpty
            ),
            folderPicker("folder-picker-onedrive-none", .oneDrive, .none),
        ]

        private static func folderPicker(
            _ id: String,
            _ provider: StorageFolderBrowser.Provider,
            _ state: PreviewData.FolderPickerState
        ) -> ShotScene {
            ShotScene(id: id, size: CGSize(width: 460, height: 560)) {
                ShotStage(
                    SyncSetupWizard.stagedPicker(
                        PreviewData.folderBrowser(provider, state)
                    )
                )
            }
        }
    #else
        static let folderPickerScenes: [ShotScene] = []
    #endif
}
