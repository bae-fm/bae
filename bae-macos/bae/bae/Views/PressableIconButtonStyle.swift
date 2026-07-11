import SwiftUI

/// Icon-button style with tactile press feedback: the label squeezes and dims
/// while the mouse is down. The `.plain` style gives icon buttons no pressed
/// state at all, which reads as the click not registering.
struct PressableIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                // Inset from the label's (hit-target-sized) bounds: the ring
                // marks the control, not the whole hitbox.
                Circle()
                    .fill(.white.opacity(configuration.isPressed ? 0.18 : 0))
                    .padding(4)
            )
            .scaleEffect(configuration.isPressed ? 0.78 : 1)
            // Press-down feedback is INSTANT (no animation on the way in —
            // any ease there reads as the click not registering); only the
            // release relaxes with an ease.
            .animation(
                configuration.isPressed ? nil : .easeOut(duration: 0.15),
                value: configuration.isPressed
            )
    }
}
