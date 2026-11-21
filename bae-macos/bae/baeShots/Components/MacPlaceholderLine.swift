import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPlaceholderLine`: a line, a shorter one, a table cell's and a
    /// picture's.
    static let macPlaceholderLineScenes: [ShotScene] = [
        component("mac.component.placeholder-line.line", height: 40) {
            MacPlaceholderLine(width: 140, height: 12)
        },
        component("mac.component.placeholder-line.short-line", height: 40) {
            MacPlaceholderLine(width: 90, height: 10)
        },
        component("mac.component.placeholder-line.table-cell", height: 40) {
            MacPlaceholderLine(width: 180, height: 14)
        },
        component("mac.component.placeholder-line.picture", height: 70) {
            MacPlaceholderLine(
                width: 44,
                height: 44,
                radius: ThemeRadius.artwork
            )
        },
        component("mac.component.placeholder-line.fixed", height: 40) {
            MacPlaceholderLine(width: 140, height: 12)
        },
    ]
}
