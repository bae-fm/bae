import SwiftUI

extension EnvironmentValues {
    @Entry
    public var accentChoice: AccentChoice = .standard
}

/// The accent the person chose in Settings, resolved where it is drawn so an
/// accent or appearance change repaints it. It is not `Color.accentColor`: a
/// Mac with an accent of its own set draws that in the Mac's accent.
struct ThemeAccent: ShapeStyle, Hashable {
    func resolve(in environment: EnvironmentValues) -> Color.Resolved {
        environment.accentChoice.color(in: environment.colorScheme)
            .resolve(in: environment)
    }
}
