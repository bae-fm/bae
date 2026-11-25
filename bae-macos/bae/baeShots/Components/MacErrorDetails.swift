import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacErrorDetails`, one shot for each row of its preview.
    static let macErrorDetailsScenes: [ShotScene] = [
        component(
            "mac.component.error-details.causes",
            width: 320,
            height: 48
        ) {
            MacErrorDetails(
                detail: "The sign-in settings file is missing.",
                excerpt:
                    "The sign-in settings file is missing.\nOAuthConfig: missing client id"
            )
            .frame(width: 320, alignment: .leading)
        }
    ]
}
