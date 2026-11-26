import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The menus and pop-ups open, drawn in place: bae's face with the
    /// highlight in the accent.
    static let menuScenes: [ShotScene] = [
        formatPicker,
        ShotScene(
            id: "mac.library.album-menu",
            size: CGSize(width: 1_200, height: 1_020),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let uiStore = UiStore()
            let detailAnswered = AlbumDetailAnswered()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 4,
                openAlbum: true,
                detailAnswered: detailAnswered
            )
            let images = coverGalleryImages()
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        images: images,
                        size: CGSize(width: 1_200, height: 1_020)
                    )
                    MacMenuInPlace(
                        {
                            AlbumMenu(
                                offersSetAsPrimary: false,
                                rating: .stars(mine: nil, raters: nil),
                                onRate: { _ in },
                                onSetHearted: { _ in },
                                onShuffle: {},
                                onAddNext: {},
                                onAddToQueue: {},
                                onEdit: {},
                                onOpenStorage: {},
                                onExport: {},
                                onSaveAs: {},
                                onSetPrimaryRelease: {},
                                onDeleteRelease: {},
                                onDeleteAlbum: {}
                            )
                        },
                        highlighted: 7
                    )
                    // The board puts the menu 330 pt down from the window's
                    // content, which starts 55 pt below the window's top.
                    .padding(.leading, 639.5)
                    .padding(.top, 386)
                },
                images: images
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
                    && detailAnswered.answered
            }
        },
    ]

    /// A format's edit sheet attached to the Settings window on Save as, with
    /// the Format pop-up open over its row. The open menu is a window of its
    /// own over the sheet's: its left edge is 363 points in from the sheet's,
    /// and its top edge 22 above the sheet's top.
    private static var formatPicker: ShotScene {
        ShotScene(
            id: "mac.settings.save-as.format-picker",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let configStore = PreviewData.configStore()
            let window = SettingsWindowShot(
                tab: .formats,
                page: FormatsSettingsTab()
                    .environment(configStore)
                    .environment(Outputs.stub())
                    .environment(UiStore())
            )
            let sheet = SavePresetEditor(
                preset: PreviewData.savePresets[1],
                presets: PreviewData.savePresets,
                update: { _ in },
                rename: { _ in .saved }
            )
            .environment(UiStore())
            let menu = MacMenuInPlace(
                {
                    MacMenu(
                        SavePresetEditor.formatOptions,
                        selection: PreviewData.savePresets[1].codec.format,
                        select: { _ in }
                    )
                    .entries
                },
                highlighted: 4,
                isPopUp: true
            )
            return ShotStage(
                OverWindow.attachedSheet(
                    OverWindow.menu(
                        menu,
                        at: CGPoint(x: 363, y: -22),
                        size: CGSize(width: 85, height: 155),
                        overSheet: sheet
                    ),
                    over: window
                ),
                // The menu's window opens after the sheet's content is up.
                isDrawn: { host in
                    host.window?.attachedSheet?.childWindows?.isEmpty == false
                }
            )
            .awaitingSheet(atRest: true)
        }
    }
}
