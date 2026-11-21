import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacRestoreCode` hidden, shown, and for an S3 library, whose key the
    /// code does not carry.
    static let macRestoreCodeScenes: [ShotScene] = [
        restoreCode(
            "mac.component.restore-code.hidden",
            height: 150,
            state: .masked(PreviewData.restoreCodeShare)
        ),
        restoreCode(
            "mac.component.restore-code.shown",
            height: 170,
            state: .shown(PreviewData.restoreCodeShare)
        ),
        restoreCode(
            "mac.component.restore-code.s3",
            height: 190,
            state: .masked(PreviewData.restoreCodeShare),
            storage: .s3(
                bucket: "my-bucket",
                region: "us-east-1",
                endpoint: "https://s3.home.lan",
                profile: nil
            )
        ),
    ]

    private static func restoreCode(
        _ id: String,
        height: CGFloat,
        state: RestoreCodeReveal.State,
        storage: BridgeStorageLocation? = nil
    ) -> ShotScene {
        component(id, width: 484, height: height) {
            ShotForm {
                MacRestoreCode(
                    reveal: RestoreCodeReveal(previewState: state),
                    onPrint: { _ in }
                )
            }
            .environment(
                LibrarySettingsTabPreviewFixture(
                    storage: storage ?? PreviewData.previewDropboxStorage
                )
                .configStore
            )
        }
    }
}
