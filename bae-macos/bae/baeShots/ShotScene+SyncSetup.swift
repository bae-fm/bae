import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Set up sync's sheet while S3 setup runs: connecting, with Cancel;
    /// creating the library, past its point of no return, without it; and a
    /// cancel under way until coven has undone the setup.
    static let syncSetupScenes: [ShotScene] = [
        syncSetup("sync-setup-s3-connecting", .running(.connecting)),
        syncSetup("sync-setup-s3-creating", .finishing(.creatingStore)),
        syncSetup("sync-setup-s3-cancelling", .cancelling),
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
