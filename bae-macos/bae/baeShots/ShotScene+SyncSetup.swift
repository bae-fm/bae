import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Set up sync's sheet while S3 setup runs: connecting, with Cancel;
    /// creating the library, past its point of no return, without it; and a
    /// cancel under way until coven has undone the setup. Then the form as
    /// typed after a failure, with the reason under its field.
    static let syncSetupScenes: [ShotScene] =
        signingInScenes + [
            ShotScene(
                id: "sync-setup-s3-restore-code",
                size: CGSize(width: 460, height: 560)
            ) {
                ShotStage(
                    SyncSetupWizard.finished(
                        on: .s3,
                        holding: .shown(code: PreviewData.restoreCode)
                    )
                )
            },
            ShotScene(
                id: "sync-setup-choose",
                size: CGSize(width: 460, height: 560)
            ) {
                ShotStage(SyncSetupWizard.choosing())
            },
            ShotScene(
                id: "sync-setup-icloud",
                size: CGSize(width: 460, height: 560)
            ) {
                ShotStage(SyncSetupWizard.iCloud())
            },
            syncSetup("sync-setup-s3-connecting", .running(.connecting)),
            syncSetup("sync-setup-s3-creating", .finishing(.creatingStore)),
            syncSetup("sync-setup-s3-cancelling", .cancelling),
            syncSetup(
                "sync-setup-s3-region-mismatch",
                .failed(.regionMismatch)
            ),
            syncSetup(
                "sync-setup-s3-bucket-not-found",
                .failed(.containerNotFound)
            ),
            syncSetup(
                "sync-setup-s3-key-not-accepted",
                .refusedKey(.notAccepted)
            ),
            syncSetup(
                "sync-setup-s3-key-cannot-use-bucket",
                .refusedKey(.cannotUseBucket)
            ),
            syncSetup(
                "sync-setup-s3-bucket-not-empty",
                .failed(.ownLocationRequired)
            ),
        ]

    /// Dropbox's screen while the person signs in in their browser, which
    /// only the full edition has.
    private static var signingInScenes: [ShotScene] {
        #if BAE_OAUTH_PROVIDERS
            [
                ShotScene(
                    id: "sync-setup-dropbox-signing-in",
                    size: CGSize(width: 460, height: 560)
                ) {
                    ShotStage(SyncSetupWizard.signingInToDropbox())
                }
            ]
        #else
            []
        #endif
    }

    private static func syncSetup(
        _ id: String,
        _ staging: SyncSetupWizard.SetupStaging
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 460, height: 560)) {
            ShotStage(SyncSetupWizard.staged(staging))
        }
    }
}
