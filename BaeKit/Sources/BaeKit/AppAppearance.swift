import SwiftUI

extension EnvironmentValues {
    @Entry
    public var accentChoice: AccentChoice = .blue
}

private struct AppAppearance: ViewModifier {
    private var preferences = AppearancePreferences()

    func body(content: Content) -> some View {
        content.appearance(
            mode: preferences.mode,
            accent: preferences.accent,
            tone: preferences.tone
        )
    }
}

private struct AppearanceStyle: ViewModifier {
    let mode: AppearanceMode
    let accent: AccentChoice
    let tone: SurfaceTone

    @Environment(\.colorScheme)
    private var colorScheme

    func body(content: Content) -> some View {
        content
            .environment(\.surfaceTone, tone)
            .environment(\.accentChoice, accent)
            .accentColor(accent.color(in: mode.colorScheme ?? colorScheme))
            .tint(accent.color(in: mode.colorScheme ?? colorScheme))
            .preferredColorScheme(mode.colorScheme)
    }
}

extension View {
    /// Apply persisted appearance to each application window or scene.
    public func appAppearance() -> some View {
        modifier(AppAppearance())
    }

    /// An explicit appearance for previews and captures, without preference writes.
    public func appearance(
        mode: AppearanceMode,
        accent: AccentChoice,
        tone: SurfaceTone
    ) -> some View {
        modifier(AppearanceStyle(mode: mode, accent: accent, tone: tone))
    }
}
