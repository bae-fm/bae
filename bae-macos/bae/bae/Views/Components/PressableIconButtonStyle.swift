import BaeKit
import SwiftUI

/// Icon-button press feedback: a circle fill behind the label and a slight
/// squeeze while the mouse is down, since `.plain` shows no pressed state.
struct PressableIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                // Inset so the fill marks the control, not the whole hit
                // target.
                Circle()
                    .fill(
                        configuration.isPressed ? Theme.pressed : Color.clear
                    )
                    .padding(ThemeSpace.inline)
            )
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            // Instant on press, since an ease there reads as a missed click;
            // only the release eases.
            .animation(
                configuration.isPressed ? nil : .easeOut(duration: 0.15),
                value: configuration.isPressed
            )
    }
}
