import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacS3KeyFields` empty, typed in and refused. The ID field takes the
    /// focus while it is empty and the secret field once the ID is typed, so
    /// the preview's "ID focused" with both typed has no state of the view.
    static let macS3KeyFieldsScenes: [ShotScene] = [
        s3KeyFields("mac.component.s3-key-fields.empty", height: 130),
        s3KeyFields(
            "mac.component.s3-key-fields.typed",
            height: 130,
            accessKey: "AKIA4EXAMPLENEW",
            secretKey: "abcdefghijkl"
        ),
        s3KeyFields(
            "mac.component.s3-key-fields.refused",
            height: 200,
            accessKey: "AKIA4EXAMPLENEW",
            secretKey: "abcdefghijkl",
            refused: true
        ),
        s3KeyFields(
            "mac.component.s3-key-fields.id-focused",
            height: 130,
            accessKey: "AKIA4EXAMPLENEW",
            secretKey: "abcdefghijkl"
        ),
    ]

    private static func s3KeyFields(
        _ id: String,
        height: CGFloat,
        accessKey: String = "",
        secretKey: String = "",
        refused: Bool = false
    ) -> ShotScene {
        component(id, width: 404, height: height) {
            MacS3KeyFields(
                accessKey: .constant(accessKey),
                secretKey: .constant(secretKey)
            ) {
                if refused {
                    MacNotice(
                        title: String(localized: "Could not use this key"),
                        advice: String(
                            localized:
                                "S3 storage does not accept this access key."
                        ),
                        error: nil,
                        look: .quiet
                    )
                }
            }
        }
    }
}
