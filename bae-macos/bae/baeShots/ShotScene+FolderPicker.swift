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

        private enum PickerState {
            case list, naming, made, notEmpty, none
        }

        private static func folderPicker(
            _ id: String,
            _ provider: StorageFolderBrowser.Provider,
            _ state: PickerState
        ) -> ShotScene {
            ShotScene(id: id, size: CGSize(width: 460, height: 560)) {
                ShotStage(
                    SyncSetupWizard.stagedPicker(
                        stagedBrowser(provider, state)
                    )
                )
            }
        }

        @MainActor
        private static func stagedBrowser(
            _ provider: StorageFolderBrowser.Provider,
            _ state: PickerState
        ) -> StorageFolderBrowser {
            func folder(_ name: String) -> BridgeStorageFolder {
                BridgeStorageFolder(
                    name: name,
                    location: provider == .dropbox
                        ? .dropbox(
                            location: .sharedFolder(namespaceId: name),
                            accountLabel: nil,
                            folderLabel: nil
                        )
                        : .oneDrive(
                            driveId: "drive",
                            folderId: name,
                            accountLabel: nil,
                            folderLabel: nil
                        )
                )
            }
            let names =
                provider == .dropbox
                ? ["Family", "Music", "Photos"] : ["Live", "Albums", "Mixes"]
            let path =
                provider == .dropbox
                ? [] : [folder("OneDrive"), folder("Music")]
            let folders = state == .none ? [] : names.map(folder)
            let listing = StorageFolderBrowser.Listing(
                path: path,
                folders: state == .made ? folders + [folder("bae")] : folders
            )
            let selected: BridgeStorageFolder? =
                switch state {
                case .list, .notEmpty: folder(names[1])
                case .made: folder("bae")
                case .naming, .none: nil
                }
            return StorageFolderBrowser(
                staged: provider,
                listing: listing,
                nests: provider == .oneDrive,
                selected: selected,
                isNaming: state == .naming,
                newFolderName: state == .naming ? "bae" : "",
                notEmpty: state == .notEmpty ? folder(names[1]) : nil
            )
        }
    #else
        static let folderPickerScenes: [ShotScene] = []
    #endif
}
