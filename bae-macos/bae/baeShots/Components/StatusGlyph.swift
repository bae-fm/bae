import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `StatusGlyph`, one shot for each row of its preview that the Mac draws:
    /// each tone's SF Symbol at the medium icon role, as a row beside body
    /// text sets it. The preview's Android, Ring and Inline rows are
    /// Android's glyphs.
    static let statusGlyphScenes: [ShotScene] = [
        component("mac.component.status-glyph.macos", height: 60) {
            HStack(spacing: 16) {
                StatusGlyph(.info)
                StatusGlyph(.success)
                StatusGlyph(.warning)
                StatusGlyph(.danger)
            }
            .themeIcon(.medium)
        },
        component("mac.component.status-glyph.muted", height: 60) {
            HStack(spacing: 16) {
                StatusGlyph(.info, muted: true)
                StatusGlyph(.success, muted: true)
                StatusGlyph(.warning, muted: true)
                StatusGlyph(.danger, muted: true)
            }
            .themeIcon(.medium)
        },
    ]
}
