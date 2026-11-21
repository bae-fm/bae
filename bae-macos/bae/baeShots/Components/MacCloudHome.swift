import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacCloudHome` for each provider, with the key replaced, with a
    /// Dropbox folder not yet known, and bare in a box beside another row.
    static let macCloudHomeScenes: [ShotScene] = [
        cloudHome(
            "mac.component.cloud-home.s3",
            s3Home,
            key: key("AKIA4EXAMPLEOLD"),
            height: 230
        ),
        cloudHome(
            "mac.component.cloud-home.s3-key-replaced",
            s3Home,
            key: key(
                "AKIA4EXAMPLENEW",
                line: .saved(oldKey: "AKIA4EXAMPLEOLD")
            ),
            height: 330
        ),
        cloudHome(
            "mac.component.cloud-home.dropbox",
            dropboxHome,
            height: 230
        ),
        cloudHome(
            "mac.component.cloud-home.onedrive",
            oneDriveStorage,
            height: 230
        ),
        cloudHome(
            "mac.component.cloud-home.dropbox-folder-unknown",
            .dropbox(location: .appFolder, accountLabel: nil, folderLabel: nil),
            height: 130
        ),
        cloudHome(
            "mac.component.cloud-home.icloud",
            .cloudKit(container: "iCloud.fm.bae", owner: "owner", zone: "zone"),
            height: 130
        ),
        component("mac.component.cloud-home.bare-in-a-box", height: 190) {
            ShotBox {
                MacRow("Status", value: "Synced", tone: .success)
                MacCloudHome(
                    location: dropboxHome,
                    shown: .bare
                )
            }
        },
    ]

    private static func cloudHome(
        _ id: String,
        _ location: BridgeStorageLocation,
        key: MacCloudHome.Key? = nil,
        height: CGFloat = 190
    ) -> ShotScene {
        component(id, height: height) {
            ShotForm {
                MacCloudHome(location: location, shown: .section(key: key))
            }
        }
    }

    private static func key(
        _ id: String,
        line: S3KeyReplacement.Line? = nil
    ) -> MacCloudHome.Key {
        MacCloudHome.Key(
            id: id,
            line: line,
            onReplace: {},
            onDismissLine: {}
        )
    }

    /// The preview's S3 library, on a home server.
    private static let s3Home = BridgeStorageLocation.s3(
        bucket: "my-bucket",
        region: "us-east-1",
        endpoint: "https://s3.home.lan",
        profile: nil
    )

    private static let dropboxHome = BridgeStorageLocation.dropbox(
        location: .sharedFolder(namespaceId: "1234567890"),
        accountLabel: "mara.quill@example.com",
        folderLabel: BridgeFolderLabel(
            name: "Music",
            webUrl: "https://www.dropbox.com/home/Music"
        )
    )

    private static let oneDriveStorage = BridgeStorageLocation.oneDrive(
        driveId: "drive-1",
        folderId: "folder-1",
        accountLabel: "mara.quill@example.com",
        folderLabel: BridgeFolderLabel(
            name: "Music Library",
            webUrl: "https://onedrive.live.com/?id=folder-1"
        )
    )
}
