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
            size: CGSize(width: 1_200, height: 1_000)
        ) {
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore)
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        size: CGSize(width: 1_200, height: 1_000)
                    )
                    MacMenuInPlace(
                        {
                            AlbumMenu(
                                offersSetAsPrimary: false,
                                onPlay: {},
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
                    .padding(.leading, 640)
                    .padding(.top, 330)
                }
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        },
        component("mac.component.popover.default", width: 284, height: 150) {
            MacPopoverInPlace(arrowSide: .top, arrowOffset: 150) {
                VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                    Text("Living Room")
                        .themeText(.strong)
                    Text("Playing on this speaker.")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
                .padding(ThemeSpace.group)
                .frame(width: 220, alignment: .leading)
            }
        },
    ]

    /// A format's edit sheet over the Settings window on Save as, with the
    /// Format pop-up open over its row: the sheet hangs 36 points from the
    /// top, and the menu's left edge is 468 points in from the window's.
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
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    OverWindow.sheet(sheet, over: window)
                    MacMenuInPlace(
                        {
                            MacMenu(
                                SavePresetEditor.formatOptions,
                                selection: PreviewData.savePresets[1].codec
                                    .format,
                                select: { _ in }
                            )
                            .entries
                        },
                        highlighted: 4,
                        isPopUp: true
                    )
                    .padding(.leading, 468)
                    .padding(.top, 14)
                }
            )
        }
    }
}
