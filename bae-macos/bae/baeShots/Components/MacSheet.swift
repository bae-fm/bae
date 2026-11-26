import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSheet`, one shot for each row of its preview.
    static let macSheetScenes: [ShotScene] = [
        // A real sheet is a window of its own, which a component's shot of
        // its view alone leaves out, so these rows are shot in a window the
        // size of the row's cell on the preview.
        ShotScene(
            id: "mac.component.sheet.default",
            size: sheetRowSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            attachedCloudHomeSheet()
        },
        component("mac.component.sheet.without-scrim", width: 600, height: 240)
        {
            MacSheet("Set up a cloud home", width: 460) {
                PreviewSlot("Content")
            } footer: {
                MacSheetFooter {
                    MacButton("Cancel") {}
                    MacButton("Set up", kind: .primary) {}
                }
            }
            // The board's sheet hangs 36 points under the top of its row,
            // as one dropped from a window's title bar does.
            .padding(.top, 36)
        },
    ]

    /// The cell of the sheet's preview rows with a scrim: as wide as the
    /// preview's column, 240 points tall.
    private static let sheetRowSize = CGSize(width: 874, height: 240)

    /// The cloud home sheet attached to a window drawn in the background
    /// colour, as the default row shows it.
    @MainActor
    private static func attachedCloudHomeSheet() -> ShotStage {
        ShotStage(
            OverWindow.attachedSheet(
                cloudHomeSheet(),
                over: Rectangle().fill(Theme.background)
            )
        )
        .awaitingSheet()
    }

    /// The sheet the default row shows.
    private static func cloudHomeSheet() -> some View {
        MacSheet("Set up a cloud home", width: 460) {
            Text(verbatim: "Content")
                .foregroundStyle(Theme.textSecondary)
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
        }
    }
}
