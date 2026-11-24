import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Theme accent in a hosted window")
@MainActor
struct ThemeAccentHostedTests {
    /// `Theme.accent` in a window drawn in a chosen accent is that accent, not
    /// the accent colour the Mac is set to. The chosen accents here are ones no
    /// Mac offers, so a Mac's own accent never passes by chance.
    @Test(
        "a view filled with Theme.accent draws the window's chosen accent",
        arguments: [AccentChoice.mint, .orchid]
    )
    func themeAccentIsTheChosenAccent(accent: AccentChoice) async throws {
        let drawn = try await centrePixel(
            of: Rectangle().fill(Theme.accent),
            accent: accent
        )
        let wanted = try await centrePixel(
            of: Rectangle().fill(accent.color(in: .dark)),
            accent: accent
        )
        #expect(abs(drawn.redComponent - wanted.redComponent) < 0.02)
        #expect(abs(drawn.greenComponent - wanted.greenComponent) < 0.02)
        #expect(abs(drawn.blueComponent - wanted.blueComponent) < 0.02)
    }

    private func centrePixel<Content: View>(
        of content: Content,
        accent: AccentChoice
    ) async throws -> NSColor {
        let view = content.appearance(
            mode: .dark,
            accent: accent,
            tone: .midnight
        )
        return try await SnapshotTestSupport.withHostedWindow(
            view,
            size: NSSize(width: 40, height: 40)
        ) { window, host in
            window.appearance = NSAppearance(named: .darkAqua)
            try await SnapshotTestSupport.settle(host)
            let bitmap = try #require(
                host.bitmapImageRepForCachingDisplay(in: host.bounds)
            )
            host.cacheDisplay(in: host.bounds, to: bitmap)
            return try #require(
                bitmap.colorAt(
                    x: bitmap.pixelsWide / 2,
                    y: bitmap.pixelsHigh / 2
                )?
                .usingColorSpace(.sRGB)
            )
        }
    }
}
