import SwiftUI

/// The layers a push button lays over its fill: the design's pressed overlay
/// while it is pressed, and its hover overlay while the pointer is over an
/// enabled button.
private struct MacButtonStateLayers: ViewModifier {
    let shape: AnyShape
    let isPressed: Bool
    let enabled: Bool

    @State
    private var hovering = false

    func body(content: Content) -> some View {
        content
            .overlay {
                shape.fill(Theme.pressed).opacity(isPressed ? 1 : 0)
            }
            .overlay {
                shape.fill(Theme.hover)
                    .opacity(!isPressed && hovering && enabled ? 1 : 0)
            }
            .onHover { hovering = $0 }
    }
}

extension View {
    func macButtonStateLayers(
        _ shape: AnyShape,
        isPressed: Bool,
        enabled: Bool
    ) -> some View {
        modifier(
            MacButtonStateLayers(
                shape: shape,
                isPressed: isPressed,
                enabled: enabled
            )
        )
    }
}
