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
                MacSheet("Set up sync", width: 460) {
                    Text(verbatim: "Content")
                        .foregroundStyle(.secondary)
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
                        Button("Cancel") {}
                        Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
                    }
                },
                over: Rectangle().fill(Theme.background)
            )
        }
    ]
}
