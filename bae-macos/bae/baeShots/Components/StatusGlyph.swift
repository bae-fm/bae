import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `StatusGlyph`, one shot for each row of its preview that the Mac draws:
    /// each tone's SF Symbol at the large icon role, in the role's weight or
    /// the weight the row names. The preview's Android, Ring and Inline rows
    /// are Android's glyphs.
    static let statusGlyphScenes: [ShotScene] = [
        component("mac.component.status-glyph.macos", height: 60) {
            StatusGlyphRow().themeIcon(.large)
        },
        component("mac.component.status-glyph.regular", height: 60) {
            StatusGlyphRow()
                .font(.system(size: ThemeIcon.large.size, weight: .regular))
        },
        component("mac.component.status-glyph.semibold", height: 60) {
            StatusGlyphRow()
                .font(.system(size: ThemeIcon.large.size, weight: .semibold))
        },
        component("mac.component.status-glyph.framed", height: 60) {
            StatusGlyphRow(frame: 24).themeIcon(.large)
        },
        component("mac.component.status-glyph.muted", height: 60) {
            StatusGlyphRow(muted: true).themeIcon(.large)
        },
    ]
}

/// The preview's four tones side by side, 16 apart, each in a `frame` square
/// when the row sets one.
private struct StatusGlyphRow: View {
    var muted = false
    var frame: CGFloat?

    var body: some View {
        HStack(spacing: 16) {
            glyph(.info)
            glyph(.success)
            glyph(.warning)
            glyph(.danger)
        }
    }

    private func glyph(_ tone: StatusTone) -> some View {
        StatusGlyph(tone, muted: muted)
            .frame(width: frame, height: frame)
    }
}
