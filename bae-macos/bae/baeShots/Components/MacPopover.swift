import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPopover`, one shot for each row of its preview: hanging below its
    /// control, opening above it, with no padding for content that pads
    /// itself, and with its arrow in the middle.
    static let macPopoverScenes: [ShotScene] = [
        popover("mac.component.popover.default", arrowSide: .top),
        popover("mac.component.popover.above", arrowSide: .bottom),
        popover("mac.component.popover.flush", arrowSide: .top, padded: false),
        popover(
            "mac.component.popover.arrow-in-the-middle",
            arrowSide: .top,
            arrowFromTrailing: 140
        ),
    ]

    /// The popover as its preview row draws it: 280 points wide, its body 10
    /// points from the row's top (or bottom, opening upward) and flush with
    /// its right edge, around the preview's placeholder for content.
    private static func popover(
        _ id: String,
        arrowSide: Edge,
        arrowFromTrailing: CGFloat = 40,
        padded: Bool = true
    ) -> ShotScene {
        let bodyWidth: CGFloat = padded ? 280 + 2 * 14 : 280
        // BaeKit's `MacPopoverFace` draws a 7 point arrow and a room of 40
        // points for the shadow around the body.
        let arrowLength: CGFloat = 7
        let shadowRoom: CGFloat = 40
        let distance: CGFloat = 10
        return component(id, width: 842, height: 160) {
            ZStack(
                alignment: arrowSide == .top ? .topTrailing : .bottomTrailing
            ) {
                Color.clear
                MacPopoverInPlace(
                    arrowSide: arrowSide,
                    arrowOffset: bodyWidth - arrowFromTrailing
                ) {
                    Text("Popover content")
                        .themeText(.body)
                        .foregroundStyle(Theme.textSecondary)
                        .padding(ThemeSpace.group + 1)
                        .frame(width: 280, alignment: .leading)
                        .padding(padded ? 14 : 0)
                }
                .padding(
                    arrowSide == .top ? .top : .bottom,
                    distance - arrowLength - shadowRoom
                )
                .padding(.trailing, -shadowRoom)
            }
        }
    }
}
