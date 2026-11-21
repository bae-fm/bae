import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAvatarButton`, one shot for each row of its preview: the person's
    /// initials and their picture.
    static let macAvatarButtonScenes: [ShotScene] = [
        avatarButton("mac.component.avatar-button.initials"),
        avatarButton("mac.component.avatar-button.picture"),
    ]

    private static func avatarButton(_ id: String) -> ShotScene {
        component(
            id,
            height: 50,
            images: ImageStore(fetchAvatarBytes: {
                PreviewData.previewArtData($0)
            })
        ) {
            MacAvatarButton()
                .environment(
                    MembershipStore.stub(
                        people: PreviewData.members(
                            reaching: PreviewData.previewDropboxStorage
                        ),
                        selfIsAdmin: true
                    )
                )
                .environment(PreviewData.syncStatus(hasCloudHome: true))
                .environment(UiStore())
                .environment(SettingsNavigation())
        }
    }
}
