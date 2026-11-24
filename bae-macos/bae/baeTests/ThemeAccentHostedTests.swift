import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Theme accent in a hosted window")
@MainActor
struct ThemeAccentHostedTests {
    /// `Theme.accent` is the accent the window is drawn in, and not the accent
    /// SwiftUI holds, which a Mac with an accent of its own set replaces. The
    /// test fills SwiftUI's accent and tint with red, which no accent of bae's
    /// is, so a view that reads either draws red or the Mac's accent.
    @Test(
        "a view filled with Theme.accent draws the window's chosen accent",
        arguments: [AccentChoice.mint, .orchid]
    )
    func themeAccentIsTheChosenAccent(accent: AccentChoice) async throws {
        let drawn = try await centrePixel(
            of: Rectangle().fill(Theme.accent)
                .accentColor(.red)
                .tint(.red),
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
