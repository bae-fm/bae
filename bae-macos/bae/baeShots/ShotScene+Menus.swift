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
                            MacMenuItem("Play") {}
                            MacMenuItem("Shuffle") {}
                            MacMenuDivider()
                            MacMenuItem("Play next") {}
                            MacMenuItem("Add to queue") {}
                            MacMenuDivider()
                            MacMenuItem("Edit…") {}
                            MacMenuItem("Storage…") {}
                            MacMenuItem("Export…") {}
                            MacMenuItem("Save as…") {}
                            MacMenuDivider()
                            MacMenuItem("Delete release") {}
                            MacMenuItem("Delete album…") {}
                        },
                        highlighted: 7
                    )
                    .padding(.leading, 640)
                    .padding(.top, 330)
                }
            ) {
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        },
        component("mac.component.popover.default", width: 300, height: 150) {
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
                update: { _ in }
            )
            .environment(UiStore())
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    OverWindow.sheet(sheet, over: window)
                    MacMenuInPlace(
                        {
                            for name in ["FLAC", "WAV", "AIFF"] {
                                MacMenuItem(verbatim: name, checked: false) {}
                            }
                            MacMenuDivider()
                            for name in ["MP3", "AAC", "Opus"] {
                                MacMenuItem(
                                    verbatim: name,
                                    checked: name == "MP3"
                                ) {}
                            }
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
