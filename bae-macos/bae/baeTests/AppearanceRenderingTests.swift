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
                        .usingColorSpace(image.colorSpace)
                )
                #expect(
                    distance(actual, expected) < 0.02,
                    "\(mode) \(tone): \(actual) expected \(expected)"
                )
            }
        }
    }

    /// The segmented mode picker fills its selected segment with the accent's
    /// button fill, which carries white labels, not the lighter accent used
    /// for text.
    ///
    /// Stated as the colour the control is handed rather than read back from
    /// its pixels: AppKit paints a segment's fill only while its window is
    /// key in the active app, which a test host behind whatever the person
    /// running the suite is working in never is, and which SwiftUI's
    /// `controlActiveState` does not stand in for.
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
        let (window, host) = SnapshotTestSupport.hostInWindow(
            AppearanceSettingsTab().appAppearance()
                .defaultAppStorage(defaults),
            size: size
        )
        window.isReleasedWhenClosed = false
        defer {
            window.contentView = nil
            window.close()
        }
        await SnapshotTestSupport.settle(host)
        let picker = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSSegmentedControl }
                .first
        )
        let space = try #require(NSColorSpace.extendedSRGB)
        let bezel = try #require(
            picker.selectedSegmentBezelColor?.usingColorSpace(space)
        )
        let fill = try #require(
            NSColor(AccentChoice.blue.buttonColor).usingColorSpace(space)
        )
        let text = try #require(
            NSColor(AccentChoice.blue.color(in: .dark)).usingColorSpace(space)
        )
        #expect(distance(bezel, fill) < 0.01, "bezel \(bezel), fill \(fill)")
        #expect(distance(bezel, text) > 0.03, "bezel \(bezel), text \(text)")
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
            NSColor(AccentChoice.teal.color(in: .dark))
                .usingColorSpace(image.colorSpace)
        )
        #expect(distance(actual, expected) < 0.03)
    }

    private func bitmap(_ host: NSView, size: NSSize) async throws
        -> NSBitmapImageRep
    {
        await SnapshotTestSupport.settle(host)
        try await Task.sleep(for: .milliseconds(250))
        return try SnapshotTestSupport.bitmap(of: host, size: size)
    }

    /// How far apart two colours' components are. Both must be in the
    /// capture's colour space: a capture keeps the display's space, and the
    /// same colour has other components in sRGB than in Display P3.
    private func distance(_ a: NSColor, _ b: NSColor) -> CGFloat {
        max(
            abs(a.redComponent - b.redComponent),
            abs(a.greenComponent - b.greenComponent),
            abs(a.blueComponent - b.blueComponent)
        )
    }
}
