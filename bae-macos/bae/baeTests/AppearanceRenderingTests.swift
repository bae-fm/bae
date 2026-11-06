import AppKit
import BaeKit
import Observation
import SwiftUI
import Testing

@testable import bae

@Suite("Appearance rendering", .serialized)
@MainActor
struct AppearanceRenderingTests {
    /// The stored mode and tone reach an open view's environment, which
    /// every themed colour resolves in; how each tone looks is the shots',
    /// which render every scene in every tone.
    @Test("Changing persisted tones reaches an open light and dark view")
    func liveBackgroundTones() async throws {
        let suite = "fm.bae.tests.appearance-rendering"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set("light", forKey: "appearance.mode")
        defaults.set("neutral", forKey: "appearance.tone")
        let seen = SeenAppearance()
        let size = NSSize(width: 100, height: 100)
        try await SnapshotTestSupport.withHostedWindow(
            AppearanceProbe(seen: seen)
                .appAppearance().defaultAppStorage(defaults),
            size: size
        ) { _, host in
            for mode in ["light", "dark"] {
                defaults.set(mode, forKey: "appearance.mode")
                let scheme: ColorScheme = mode == "dark" ? .dark : .light
                for tone in SurfaceTone.allCases {
                    defaults.set(tone.rawValue, forKey: "appearance.tone")
                    // The stored tone reaches the view on a later pass.
                    try await Wait.untilLaidOut(host) {
                        seen.tone == tone && seen.scheme == scheme
                    }
                }
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
        try await SnapshotTestSupport.withHostedWindow(
            AppearanceSettingsTab().appAppearance()
                .defaultAppStorage(defaults),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
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
                NSColor(AccentChoice.blue.color(in: .dark))
                    .usingColorSpace(space)
            )
            #expect(
                distance(bezel, fill) < 0.01,
                "bezel \(bezel), fill \(fill)"
            )
            #expect(
                distance(bezel, text) > 0.03,
                "bezel \(bezel), text \(text)"
            )
        }
    }

    /// How far apart two colours' components are, both in one colour space.
    private func distance(_ a: NSColor, _ b: NSColor) -> CGFloat {
        max(
            abs(a.redComponent - b.redComponent),
            abs(a.greenComponent - b.greenComponent),
            abs(a.blueComponent - b.blueComponent)
        )
    }
}

/// The appearance an open view was last handed.
@MainActor
@Observable
private final class SeenAppearance {
    var tone: SurfaceTone?
    var scheme: ColorScheme?
}

/// Records the mode and tone its environment carries as they change.
private struct AppearanceProbe: View {
    let seen: SeenAppearance
    @Environment(\.surfaceTone)
    private var tone
    @Environment(\.colorScheme)
    private var scheme

    var body: some View {
        Rectangle().fill(Theme.background)
            .onChange(of: tone, initial: true) { _, now in seen.tone = now }
            .onChange(of: scheme, initial: true) { _, now in seen.scheme = now }
    }
}
