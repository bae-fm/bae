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
