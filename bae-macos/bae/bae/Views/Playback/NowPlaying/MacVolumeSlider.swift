import BaeKit
import SwiftUI

/// A knobless volume slider. During a click or drag it shows the level under
/// the pointer and reports each one through `onDrag`. The slider sets the
/// level the gesture ends on once through `onChange` and shows that level
/// until the call returns, and then shows `value`.
struct MacVolumeSlider: View {
    let value: Float
    let onDrag: (Float) -> Void
    let onChange: (Float) async -> Void

    /// The level under the pointer, from the gesture's start until the call that sets it returns.
    @State
    private var dragValue: Float?

    /// The track's thickness.
    private static let trackHeight: CGFloat = 5

    private static func level(at x: CGFloat, width: CGFloat) -> Float {
        Float(max(0, min(1, x / width)))
    }

    private var clamped: Float { max(0, min(1, dragValue ?? value)) }

    /// Show `level` and set it through `onChange`, until that set returns.
    private func set(_ level: Float) {
        dragValue = level
        Task {
            await onChange(level)
            // When a new drag started while this call ran, keep that drag's level.
            if dragValue == level {
                dragValue = nil
            }
        }
    }

    var body: some View {
        GeometryReader { geo in
            let width = geo.size.width
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Theme.hairline)
                    .frame(height: Self.trackHeight)
                Capsule()
                    .fill(Theme.accent)
                    .frame(
                        width: CGFloat(clamped) * width,
                        height: Self.trackHeight
                    )
            }
            .frame(
                maxWidth: .infinity,
                maxHeight: .infinity,
                alignment: .leading
            )
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        guard width > 0 else { return }
                        let level = Self.level(
                            at: drag.location.x,
                            width: width
                        )
                        dragValue = level
                        onDrag(level)
                    }
                    .onEnded { drag in
                        guard width > 0 else {
                            dragValue = nil
                            return
                        }
                        let level = Self.level(
                            at: drag.location.x,
                            width: width
                        )
                        set(level)
                    },
            )
        }
        .frame(height: 20)
        .accessibilityElement(children: .ignore)
        .accessibilityValue(Text(Double(clamped).formatted(.percent)))
        .accessibilityAdjustableAction { direction in
            let step: Float = 0.05
            switch direction {
            case .increment: set(min(1, clamped + step))
            case .decrement: set(max(0, clamped - step))
            @unknown default: break
            }
        }
    }
}
