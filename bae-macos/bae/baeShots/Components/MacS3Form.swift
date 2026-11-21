import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacS3Form` with the preview's endpoint, region, bucket and key typed
    /// in, and the reason a failed setup gives under the field it names.
    static let macS3FormScenes: [ShotScene] = [
        s3Form("mac.component.s3-form.none", failing: nil),
        s3Form("mac.component.s3-form.region", failing: .regionMismatch),
        s3Form("mac.component.s3-form.bucket", failing: .containerNotFound),
        s3Form("mac.component.s3-form.occupied", failing: .ownLocationRequired),
        s3Form("mac.component.s3-form.key", failing: .authentication),
        s3Form("mac.component.s3-form.denied", failing: .permissionDenied),
    ]

    private static func s3Form(
        _ id: String,
        failing failure: BridgeStorageFailure?
    ) -> ShotScene {
        component(id, width: 520, height: 470) {
            MacS3Form(staged: s3Fields(failing: failure))
                .environment(Sync.stub())
                .environment(StorageSetupRun())
        }
    }

    @MainActor
    static func s3Fields(
        failing failure: BridgeStorageFailure?
    ) -> S3SetupFields {
        let fields = S3SetupFields()
        fields.endpoint = "https://s3.home.lan"
        fields.region = "us-east-1"
        fields.bucket = "my-bucket"
        fields.accessKey = "a1b2c3d4e5f6"
        fields.secretKey = "abcdefghijkl"
        if let failure {
            _ = fields.place(
                BridgeError.Diagnostic(
                    category: .storage(failure: failure),
                    detail: "the storage answered with an error"
                )
            )
        }
        return fields
    }
}
