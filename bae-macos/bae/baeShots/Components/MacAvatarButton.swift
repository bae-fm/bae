import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAvatarButton`, one shot for each row of its preview: the person's
    /// initials and their picture.
    static let macAvatarButtonScenes: [ShotScene] = [
        avatarButton("mac.component.avatar-button.initials", picture: false),
        avatarButton("mac.component.avatar-button.picture", picture: true),
    ]

    private static func avatarButton(
        _ id: String,
        picture: Bool
    ) -> ShotScene {
        component(
            id,
            height: 50,
            images: anaPictureStore
        ) {
            MacAvatarButton()
                .environment(
                    MembershipStore.stub(
                        people: members(withPicture: picture),
                        selfIsAdmin: true
                    )
                )
                .environment(PreviewData.syncStatus(hasCloudHome: true))
                .environment(UiStore())
                .environment(SettingsNavigation())
        }
    }

    /// The preview's people, with this device's person holding a picture when
    /// `withPicture`.
    static func members(withPicture: Bool) -> BridgePeople {
        var people = PreviewData.members(
            reaching: PreviewData.previewDropboxStorage
        )
        if withPicture {
            people.own?.profile?.avatarVersion = "1"
        }
        return people
    }

    /// The store whose avatar is the picture the board draws for Ana.
    static var anaPictureStore: ImageStore {
        ImageStore(fetchAvatarBytes: { _ in
            try BoardArtwork.data(named: "Ana")
        })
    }
}
