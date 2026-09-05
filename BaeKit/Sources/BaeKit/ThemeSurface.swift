import SwiftUI

extension EnvironmentValues {
    @Entry
    public var surfaceTone: SurfaceTone = .neutral
}

/// Resolves at the drawing site, so a tone or system appearance change
/// updates every surface without storing colors in individual views.
public struct ThemeSurface: ShapeStyle, Sendable {
    enum Role: Sendable {
        case background, surface, elevated, field, fieldHover, placeholder,
            well, tile
    }

    let role: Role

    public func resolve(in environment: EnvironmentValues) -> Color {
        guard
            let modes = AppearancePalette.bundled.tones[
                environment.surfaceTone.rawValue
            ]
        else {
            preconditionFailure(
                "Missing surface tone: \(environment.surfaceTone)"
            )
        }
        let colors = environment.colorScheme == .dark ? modes.dark : modes.light
        let hex: String
        switch role {
        case .background: hex = colors.background
        case .surface: hex = colors.surface
        case .elevated: hex = colors.elevated
        case .field: hex = colors.field
        case .fieldHover: hex = colors.fieldHover
        case .placeholder: hex = colors.placeholder
        case .well: hex = colors.well
        case .tile: hex = colors.tile
        }
        return AppearancePalette.color(hex)
    }
}
