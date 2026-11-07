import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Set up sync's sheet while S3 setup runs: connecting, with Cancel;
    /// creating the library, past its point of no return, without it; and a
    /// cancel under way until coven has undone the setup. Then the form as
    /// typed after a failure, with the reason under its field.
    static let syncSetupScenes: [ShotScene] = [
        syncSetup("sync-setup-s3-connecting", .running(.connecting)),
        syncSetup("sync-setup-s3-creating", .finishing(.creatingStore)),
        syncSetup("sync-setup-s3-cancelling", .cancelling),
        syncSetup("sync-setup-s3-region-mismatch", .failed(.regionMismatch)),
        syncSetup(
            "sync-setup-s3-bucket-not-found",
            .failed(.containerNotFound)
        ),
        syncSetup(
            "sync-setup-s3-bucket-not-empty",
            .failed(.ownLocationRequired)
        ),
    ]

    private static func syncSetup(
        _ id: String,
        _ staging: SyncSetupWizard.SetupStaging
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 460, height: 560)) {
            ShotStage(SyncSetupWizard.staged(staging))
        }
    }
}
