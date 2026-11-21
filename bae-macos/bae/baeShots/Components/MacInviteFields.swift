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
