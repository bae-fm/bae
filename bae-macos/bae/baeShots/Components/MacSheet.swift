import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSheet`, one shot for each row of its preview.
    static let macSheetScenes: [ShotScene] = [
        component(
            "mac.component.sheet.default",
            width: 600,
            height: 240
        ) {
            OverWindow.sheet(
                MacSheet("Set up a cloud home", width: 460) {
                    Text(verbatim: "Content")
                        .foregroundStyle(Theme.macTextSecondary)
                        .padding(ThemeSpace.group)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .overlay {
                            RoundedRectangle(cornerRadius: ThemeRadius.card)
                                .strokeBorder(
                                    Theme.hairlineStrong,
                                    style: StrokeStyle(
                                        lineWidth: 1,
                                        dash: [4, 3]
                                    )
                                )
                        }
                } footer: {
                    MacSheetFooter {
                        MacButton("Cancel") {}
                        MacButton("Set up", kind: .primary) {}
                    }
                },
                over: Rectangle().fill(Theme.background)
            )
        },
        component("mac.component.sheet.without-scrim", width: 600, height: 240)
        {
            MacSheet("Set up sync", width: 460) {
                Text(verbatim: "Content")
                    .foregroundStyle(Theme.macTextSecondary)
                    .padding(ThemeSpace.group)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } footer: {
                MacSheetFooter {
                    MacButton("Cancel") {}
                    MacButton("Set up", kind: .primary) {}
                }
            }
        },
    ]
}
