import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `StatusGlyph`, one shot for each row of its preview.
    static let statusGlyphScenes: [ShotScene] = [
        component("mac.component.status-glyph.macos", height: 60) {
            HStack(spacing: 16) {
                StatusGlyph(.info)
                StatusGlyph(.success)
                StatusGlyph(.warning)
                StatusGlyph(.danger)
            }
            .font(.system(size: 17))
        }
    ]
}
