import SwiftUI

extension AccentChoice {
    public func color(in scheme: ColorScheme) -> Color {
        scheme == .dark ? colors.dark : colors.light
    }

    /// The fill behind white button labels.
    public var buttonColor: Color {
        colors.fill
    }
}
