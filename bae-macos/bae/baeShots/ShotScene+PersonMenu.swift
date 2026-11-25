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
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            personMenuWindow(hasCloudHome: true, highlighted: 3)
        },
        ShotScene(
            id: "mac.library.avatar-menu.no-cloud-home-asks",
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            personMenuWindow(hasCloudHome: false, highlighted: 1)
        },
        ShotScene(
            id: "mac.library.avatar-menu.no-cloud-home-sheet",
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore)
            let images = coverGalleryImages()
            let window = personMenuLibrary(
                hasCloudHome: false,
                uiStore: uiStore,
                backing: backing,
                images: images
            )
            return ShotStage(
                OverWindow.sheet(
                    CloudHomeFirstSheet(
                        reason: .addDevice,
                        onCancel: {},
                        onSetUp: {}
                    ),
                    over: window
                )
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        },
    ]

    /// The window with the person's menu open under their picture, the item
    /// at `highlighted` under the pointer.
    private static func personMenuWindow(
        hasCloudHome: Bool,
        highlighted: Int
    ) -> ShotStage {
        let uiStore = UiStore()
        let backing = boardLibraryBacking(uiStore: uiStore)
        let images = coverGalleryImages()
        return ShotStage(
            ZStack(alignment: .topTrailing) {
                personMenuLibrary(
                    hasCloudHome: hasCloudHome,
                    uiStore: uiStore,
                    backing: backing,
                    images: images
                )
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
                .padding(.trailing, 16)
                .padding(.top, 44)
            },
            images: images
        ) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        }
    }

    /// The library window the boards draw behind the menu, Ana an admin of a
    /// library with a cloud home when `hasCloudHome` is set.
    private static func personMenuLibrary(
        hasCloudHome: Bool,
        uiStore: UiStore,
        backing: (library: Library, session: LibraryBrowseSession),
        images: ImageStore
    ) -> some View {
        ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: backing,
            playbackStore: boardPlayback(),
            images: images,
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                castEnabled: true,
                storage: hasCloudHome ? PreviewData.previewDropboxStorage : nil
            ),
            syncStatus: PreviewData.syncStatus(hasCloudHome: hasCloudHome),
            size: CGSize(width: 1200, height: 760)
        )
    }
}
