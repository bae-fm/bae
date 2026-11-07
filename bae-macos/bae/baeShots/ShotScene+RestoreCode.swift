import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The restore code in Settings, held masked and shown, and its printed
    /// page. The code is a fixture, never a library's.
    static let restoreCodeScenes: [ShotScene] = [
        restoreCodeSection("restore-code-masked", .masked(code: fixture)),
        restoreCodeSection("restore-code-shown", .shown(code: fixture)),
        ShotScene(
            id: "restore-code-print",
            size: RestoreCodePage.size
        ) {
            ShotStage(
                RestoreCodePage(
                    code: fixture,
                    location: "Dropbox: name@example.com",
                    date: Date(timeIntervalSince1970: 1_793_966_400)
                )
            )
        },
    ]

    private static let fixture = PreviewData.restoreCodeGrouped

    private static func restoreCodeSection(
        _ id: String,
        _ state: RestoreCodeReveal.State
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 200)) {
            ShotStage(
                Form {
                    RestoreCodeSection(
                        reveal: RestoreCodeReveal(previewState: state),
                        onPrint: { _ in }
                    )
                }
                .formStyle(.grouped)
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewDropboxStorage
                    )
                )
            )
        }
    }
}
