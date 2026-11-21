import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacRow`, one shot for each row of its preview.
    static let macRowScenes: [ShotScene] = [
        component("mac.component.row.label", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync") {}
            }
        },
        component("mac.component.row.value", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync", value: "Synced")
            }
        },
        component("mac.component.row.tone", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync") {
                    ToneDotValue(text: "Synced", tone: .success)
                }
            }
        },
        component("mac.component.row.opens-something", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync") {}
                    .pressing(.opens {})
            }
        },
        component("mac.component.row.strong", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync") {}
                    .strong()
            }
        },
        component("mac.component.row.tag", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync") {}
                    .chip("Member")
            }
        },
        component(
            "mac.component.row.destructive-action",
            width: 580,
            height: 70
        ) {
            ShotBox {
                MacRow("Sync") {}
                    .pressing(.removes {})
            }
        },
        component("mac.component.row.controls", width: 580, height: 70) {
            ShotBox {
                MacRow("Sync") {
                    MacButton("Set up") {}
                }
            }
        },
        pauseRows("mac.component.row.nested", off: false),
        pauseRows("mac.component.row.nested-off", off: true),
    ]

    /// The Pause row and Resume automatically under it, which is off while
    /// the pause is.
    private static func pauseRows(_ id: String, off: Bool) -> ShotScene {
        component(id, width: 580, height: 120) {
            ShotBox {
                MacRow("Pause between sides and discs") {
                    MacSwitch(
                        "Pause between sides and discs",
                        isOn: .constant(!off)
                    )
                }
                MacNested {
                    MacRow("Resume automatically") {
                        MacPopUpButton(
                            "Resume automatically",
                            selection: .constant(0)
                        ) {
                            MacPopUpOption(verbatim: "Never", value: 0)
                        }
                        .fixedSize()
                    }
                }
                .disabled(off)
            }
        }
    }
}
