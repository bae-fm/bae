import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The person's menu open under their picture, on a library with a cloud
    /// home and on one with none, and the sheet Add device opens on the
    /// latter, as the boards draw them.
    static let personMenuScenes: [ShotScene] = [
        ShotScene(
            id: "mac.library.avatar-menu",
            size: CGSize(width: 1200, height: 760)
        ) {
            personMenuWindow(hasCloudHome: true, highlighted: 3)
        },
        ShotScene(
            id: "mac.library.avatar-menu.no-cloud-home-asks",
            size: CGSize(width: 1200, height: 760)
        ) {
            personMenuWindow(hasCloudHome: false, highlighted: 1)
        },
        ShotScene(
            id: "mac.library.avatar-menu.no-cloud-home-sheet",
            size: CGSize(width: 1200, height: 760)
        ) {
            let window = personMenuLibrary(hasCloudHome: false)
            return ShotStage(
                OverWindow.sheet(
                    CloudHomeFirstSheet(
                        reason: .addDevice,
                        onCancel: {},
                        onSetUp: {}
                    ),
                    over: window
                )
            )
        },
    ]

    /// The window with the person's menu open under their picture, the item
    /// at `highlighted` under the pointer.
    private static func personMenuWindow(
        hasCloudHome: Bool,
        highlighted: Int
    ) -> ShotStage {
        ShotStage(
            ZStack(alignment: .topTrailing) {
                personMenuLibrary(hasCloudHome: hasCloudHome)
                MacMenuInPlace(
                    {
                        PersonMenuItems(
                            hasCloudHome: hasCloudHome,
                            isAdmin: true,
                            onSetUpCloudHome: {},
                            onAddDevice: {},
                            onInvite: {},
                            onPlaylists: {},
                            onHistory: {},
                            onSettings: {}
                        )
                    },
                    highlighted: highlighted
                )
                .frame(width: 200)
                .padding(.trailing, 10)
                .padding(.top, 44)
            }
        )
    }

    /// The library window the boards draw behind the menu, Ana an admin of a
    /// library with a cloud home when `hasCloudHome` is set.
    private static func personMenuLibrary(hasCloudHome: Bool) -> some View {
        let uiStore = UiStore()
        let backing = boardLibraryBacking(uiStore: uiStore)
        return ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: (backing.library, backing.session),
            playbackStore: boardPlayback(),
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                castEnabled: true,
                storage: hasCloudHome ? PreviewData.previewDropboxStorage : nil
            ),
            size: CGSize(width: 1200, height: 760)
        )
    }
}
