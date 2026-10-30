import SwiftUI

extension EnvironmentValues {
    @Entry
    public var accentChoice: AccentChoice = .standard
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
            .toned(
                tone,
                accent: accent,
                accentColor: accent.color(in: mode.colorScheme ?? colorScheme)
            )
            .preferredColorScheme(mode.colorScheme)
    }
}

/// The tone and accent of the window a view is drawn in, handed to a view
/// hosted apart from the window's own SwiftUI tree — a table cell's — which
/// inherits neither.
public struct HostedAppearance: Equatable, Sendable {
    public let tone: SurfaceTone
    public let accent: AccentChoice

    public init(tone: SurfaceTone, accent: AccentChoice) {
        self.tone = tone
        self.accent = accent
    }
}

/// Draws a hosted view in a `HostedAppearance`. Its light or dark appearance
/// it takes from the window it is in.
private struct HostedAppearanceStyle: ViewModifier {
    let appearance: HostedAppearance

    @Environment(\.colorScheme)
    private var colorScheme

    func body(content: Content) -> some View {
        content.toned(
            appearance.tone,
            accent: appearance.accent,
            accentColor: appearance.accent.color(in: colorScheme)
        )
    }
}

extension View {
    fileprivate func toned(
        _ tone: SurfaceTone,
        accent: AccentChoice,
        accentColor: Color
    ) -> some View {
        environment(\.surfaceTone, tone)
            .environment(\.accentChoice, accent)
            .accentColor(accentColor)
            .tint(accentColor)
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

    /// Draw in the tone and accent of the window around a view that is
    /// hosted apart from that window's SwiftUI tree, such as a table cell.
    public func hostedAppearance(_ appearance: HostedAppearance) -> some View {
        modifier(HostedAppearanceStyle(appearance: appearance))
    }
}
