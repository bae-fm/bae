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
            arrowOffset: 142
        ),
    ]

    private static func popover(
        _ id: String,
        arrowSide: Edge,
        arrowOffset: CGFloat = 150,
        padded: Bool = true
    ) -> ShotScene {
        component(id, width: 284, height: 150) {
            MacPopoverInPlace(arrowSide: arrowSide, arrowOffset: arrowOffset) {
                VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                    Text("Living Room")
                        .themeText(.strong)
                    Text("Playing on this speaker.")
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                }
                .padding(padded ? ThemeSpace.group : 0)
                .frame(width: 220, alignment: .leading)
            }
        }
    }
}
