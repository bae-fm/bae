import SwiftUI

/// The platform's prominent button with the palette's readable fill. Native
/// focus, keyboard, disabled, pressed, and destructive behavior remain intact.
public struct PrimaryButtonStyle: PrimitiveButtonStyle {
    @Environment(\.accentChoice)
    private var accent

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Button(configuration)
            .buttonStyle(.borderedProminent)
            .tint(accent.buttonColor)
    }
}
