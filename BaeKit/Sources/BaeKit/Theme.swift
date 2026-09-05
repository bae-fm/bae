import SwiftUI

/// Shared visual roles. Surfaces resolve the selected tone at the drawing site;
/// accents inherit the selected color from the application root.
public enum Theme {
    public static let background = ThemeSurface(role: .background)
    public static let surface = ThemeSurface(role: .surface)
    public static let placeholder = ThemeSurface(role: .placeholder)
    public static let surfaceElevated = ThemeSurface(role: .elevated)
    public static let field = ThemeSurface(role: .field)
    public static let fieldHover = ThemeSurface(role: .fieldHover)
    public static let well = ThemeSurface(role: .well)
    public static let tile = ThemeSurface(role: .tile)
    public static let accent = Color.accentColor
    public static let accentSoft = accent.opacity(0.14)
    public static let hairline = Color.primary.opacity(0.10)
    public static let hover = Color.primary.opacity(0.06)
}

extension View {
    /// The window's base surface, resolved from its appearance and tone.
    public func windowBackground() -> some View {
        background(Theme.background)
    }
}
