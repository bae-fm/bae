import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacInviteFields` for each storage, in its sheet.
    static let macInviteFieldsScenes: [ShotScene] = [
        inviteFields("mac.component.invite-fields.s3-their-key", .s3),
        inviteFields(
            "mac.component.invite-fields.s3-my-key",
            .s3,
            usesOwnKey: true
        ),
        inviteFields("mac.component.invite-fields.dropbox", .dropbox),
        inviteFields("mac.component.invite-fields.onedrive", .oneDrive),
        inviteFields("mac.component.invite-fields.icloud", .iCloud),
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

    private static func inviteFields(
        _ id: String,
        _ storage: InviteStorage,
        usesOwnKey: Bool = false
    ) -> ShotScene {
        component(id, width: 440, height: 400) {
            MacInviteFields(
                storage: storage,
                email: .constant("dan@example.com"),
                accessKeyId: .constant("AKIAEXAMPLEKEY"),
                secretAccessKey: .constant("abcdefghijkl"),
                usesOwnKey: .constant(usesOwnKey),
                role: .constant(.member),
                lifetime: .constant(.sevenDays),
                error: nil,
                canCreate: true,
                onCreate: {},
                onCancel: {}
            )
        }
    }
}
