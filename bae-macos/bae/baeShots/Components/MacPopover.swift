import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPopover`, over its anchor.
    static let macPopoverScenes: [ShotScene] = [
        component("mac.component.popover.default", width: 284, height: 150) {
            MacPopoverInPlace(arrowSide: .top, arrowOffset: 150) {
                VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                    Text("Living Room")
                        .themeText(.strong)
                    Text("Playing on this speaker.")
                        .themeText(.detail)
                        .foregroundStyle(Theme.macTextSecondary)
                }
                .padding(ThemeSpace.group)
                .frame(width: 220, alignment: .leading)
            }
        }
    ]
}
