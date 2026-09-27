import SwiftUI

extension EnvironmentValues {
    @Entry
    public var surfaceTone: SurfaceTone = .neutral
}

/// A surface of the chosen tone, resolved where it is drawn so a tone or
/// appearance change repaints it.
public struct ThemeSurface: ShapeStyle, Sendable {
    let role: KeyPath<ToneSurfaces, Color> & Sendable

    public func resolve(in environment: EnvironmentValues) -> Color {
        environment.surfaceTone
            .surfaces(dark: environment.colorScheme == .dark)[keyPath: role]
    }
}
