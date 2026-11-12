import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The menus and pop-ups open, drawn in place: bae's face with the
    /// highlight in the accent.
    static let menuScenes: [ShotScene] = [
        component("mac.settings.save-as.format-picker", width: 480, height: 230)
        {
            ZStack(alignment: .topLeading) {
                ShotBox {
                    LabeledContent("Format") {
                        MacPopUpButton("Format", selection: .constant("MP3")) {
                            MacPopUpOption(verbatim: "MP3", value: "MP3")
                        }
                    }
                    LabeledContent("Bitrate") {
                        MacPopUpButton(
                            "Bitrate",
                            selection: .constant("320 kbps")
                        ) {
                            MacPopUpOption(
                                verbatim: "320 kbps",
                                value: "320 kbps"
                            )
                        }
                    }
                }
                MacMenuInPlace(
                    {
                        for name in ["FLAC", "WAV", "AIFF"] {
                            MacMenuItem(verbatim: name, checked: false) {}
                        }
                        MacMenuDivider()
                        for name in ["MP3", "AAC", "Opus"] {
                            MacMenuItem(verbatim: name, checked: name == "MP3")
                            {}
                        }
                    },
                    highlighted: 4,
                    isPopUp: true
                )
                .offset(x: 330, y: -55)
            }
            .padding(.top, 60)
        },
        component("mac.library.album-menu", width: 260, height: 340) {
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
}
