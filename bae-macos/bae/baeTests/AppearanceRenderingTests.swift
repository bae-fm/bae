import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Appearance rendering", .serialized)
@MainActor
struct AppearanceRenderingTests {
    @Test("Changing persisted tones repaints an open light and dark view")
    func liveBackgroundTones() async throws {
        let suite = "fm.bae.tests.appearance-rendering"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set("light", forKey: "appearance.mode")
        defaults.set("neutral", forKey: "appearance.tone")
        let size = NSSize(width: 100, height: 100)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            Rectangle().fill(Theme.background)
                .appAppearance().defaultAppStorage(defaults),
            size: size
        )
        window.isReleasedWhenClosed = false
        defer {
            window.contentView = nil
            window.close()
        }
        for mode in ["light", "dark"] {
            defaults.set(mode, forKey: "appearance.mode")
            for tone in SurfaceTone.allCases {
                defaults.set(tone.rawValue, forKey: "appearance.tone")
                let image = try await bitmap(host, size: size)
                let actual = try #require(image.colorAt(x: 50, y: 50))
                var environment = EnvironmentValues()
                environment.colorScheme = mode == "dark" ? .dark : .light
                environment.surfaceTone = tone
                let expected = try #require(
                    NSColor(Theme.background.resolve(in: environment))
                        .usingColorSpace(.sRGB)
                )
                #expect(
                    distance(actual, expected) < 0.02,
                    "\(mode) \(tone): \(actual) expected \(expected)"
                )
            }
        }
    }

    @Test(
        "The selected mode uses the swatch fill, not the text accent",
        arguments: [AppearanceMode.system, .dark]
    )
    func selectedModeFill(mode: AppearanceMode) async throws {
        let suite = "fm.bae.tests.appearance-controls"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set(mode.rawValue, forKey: "appearance.mode")
        defaults.set("blue", forKey: "appearance.accent")
        let size = NSSize(width: 500, height: 300)
        let host = NSHostingView(
            rootView:
                AppearanceSettingsTab().appAppearance()
                .defaultAppStorage(defaults)
        )
        host.frame = NSRect(origin: .zero, size: size)
        let window = NSWindow(
            contentRect: host.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: .darkAqua)
        let previousPolicy = NSApp.activationPolicy()
        NSApp.setActivationPolicy(.regular)
        defer { NSApp.setActivationPolicy(previousPolicy) }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        window.makeMain()
        defer {
            window.contentView = nil
            window.close()
        }
        let image = try await bitmap(host, size: size)
        let fill = try #require(
            NSColor(AccentChoice.blue.buttonColor).usingColorSpace(.sRGB)
        )
        let text = try #require(
            NSColor(AccentChoice.blue.color(in: .dark)).usingColorSpace(.sRGB)
        )
        var fillPixels = 0
        var textPixels = 0
        for y in 0..<image.pixelsHigh {
            for x in 0..<image.pixelsWide {
                let pixel = try #require(image.colorAt(x: x, y: y))
                if distance(pixel, fill) < 0.03 { fillPixels += 1 }
                if distance(pixel, text) < 0.03 { textPixels += 1 }
            }
        }
        // The swatch alone occupies fewer than 2,000 pixels at this scale.
        // The selected segment must contribute its filled area too.
        #expect(
            fillPixels > 2000 && fillPixels > textPixels,
            "fill: \(fillPixels), text accent: \(textPixels)"
        )
    }

    @Test("Volume follows the accent used by playback progress")
    func volumeAccent() async throws {
        let size = NSSize(width: 120, height: 20)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            SlimSlider(value: 0.5, onChange: { _ in })
                .appearance(mode: .dark, accent: .teal, tone: .slate),
            size: size
        )
        window.isReleasedWhenClosed = false
        defer {
            window.contentView = nil
            window.close()
        }
        let image = try await bitmap(host, size: size)
        let actual = try #require(
            image.colorAt(x: image.pixelsWide / 4, y: image.pixelsHigh / 2)
        )
        let expected = try #require(
            NSColor(AccentChoice.teal.color(in: .dark)).usingColorSpace(.sRGB)
        )
        #expect(distance(actual, expected) < 0.03)
    }

    private func bitmap(_ host: NSView, size: NSSize) async throws
        -> NSBitmapImageRep
    {
        await SnapshotTestSupport.settle(host)
        try await Task.sleep(for: .milliseconds(250))
        let bitmap = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: Int(size.width) * 2,
                pixelsHigh: Int(size.height) * 2,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 0,
                bitsPerPixel: 0
            )?
            .retagging(with: .sRGB)
        )
        bitmap.size = size
        host.cacheDisplay(in: NSRect(origin: .zero, size: size), to: bitmap)
        return bitmap
    }

    private func distance(_ a: NSColor, _ b: NSColor) -> CGFloat {
        max(
            abs(a.redComponent - b.redComponent),
            abs(a.greenComponent - b.greenComponent),
            abs(a.blueComponent - b.blueComponent)
        )
    }
}
