import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    #if BAE_OAUTH_PROVIDERS
        /// `MacFolderPicker` in the setup sheet that holds it, with the New
        /// folder… and Look again buttons and Cancel and Set up in its
        /// footer.
        static let macFolderPickerScenes: [ShotScene] = [
            folderPicker(
                "mac.component.folder-picker.naming",
                dropbox(["Family", "Music", "Photos"]),
                isNaming: true,
                newFolderName: "bae"
            ),
            folderPicker(
                "mac.component.folder-picker.list",
                dropbox(["Family", "Music", "Photos"]),
                selected: "Music"
            ),
            folderPicker(
                "mac.component.folder-picker.made",
                dropbox(["Family", "Music", "Photos", "bae"]),
                selected: "bae"
            ),
            folderPicker(
                "mac.component.folder-picker.not-empty",
                dropbox(["Family", "Music", "Photos"]),
                selected: "Music",
                notEmpty: "Music"
            ),
            folderPicker(
                "mac.component.folder-picker.empty",
                dropbox([])
            ),
            folderPicker(
                "mac.component.folder-picker.onedrive",
                oneDrive(["Live", "Albums", "Mixes"]),
                selected: "Albums"
            ),
        ]

        private static func dropbox(_ names: [String]) -> FolderListing {
            FolderListing(
                provider: .dropbox,
                path: [],
                folders: names.map {
                    BridgeStorageFolder(
                        name: $0,
                        location: .dropbox(
                            location: .sharedFolder(namespaceId: $0),
                            accountLabel: nil,
                            folderLabel: nil
                        )
                    )
                }
            )
        }

        private static func oneDrive(_ names: [String]) -> FolderListing {
            func folder(_ name: String) -> BridgeStorageFolder {
                BridgeStorageFolder(
                    name: name,
                    location: .oneDrive(
                        driveId: "drive",
                        folderId: name,
                        accountLabel: nil,
                        folderLabel: nil
                    )
                )
            }
            return FolderListing(
                provider: .oneDrive,
                path: [folder("OneDrive"), folder("Music")],
                folders: names.map(folder)
            )
        }

        /// The folders a provider lists, and the path to them.
        private struct FolderListing {
            let provider: StorageFolderBrowser.Provider
            let path: [BridgeStorageFolder]
            let folders: [BridgeStorageFolder]
        }

        private static func folderPicker(
            _ id: String,
            _ listing: FolderListing,
            selected: String? = nil,
            isNaming: Bool = false,
            newFolderName: String = "",
            notEmpty: String? = nil
        ) -> ShotScene {
            component(id, width: 480, height: 440) {
                let named = { (name: String?) in
                    listing.folders.first { $0.name == name }
                }
                OAuthFolderSetup(
                    browser: StorageFolderBrowser(
                        staged: listing.provider,
                        listing: StorageFolderBrowser.Listing(
                            path: listing.path,
                            folders: listing.folders
                        ),
                        nests: listing.provider == .oneDrive,
                        selected: named(selected),
                        isNaming: isNaming,
                        newFolderName: newFolderName,
                        notEmpty: named(notEmpty)
                    ),
                    leave: {},
                    onConnected: { _ in }
                )
                .environment(Sync.stub())
                .environment(StorageSetupRun())
                .environment(CloudSyncSetup())
            }
        }
    #else
        static let macFolderPickerScenes: [ShotScene] = []
    #endif
}
