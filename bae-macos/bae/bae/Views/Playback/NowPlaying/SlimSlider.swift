import BaeKit
import SwiftUI

/// A knobless volume slider. During a click or drag it shows the level under
/// the pointer and reports each one through `onDrag`; the level the gesture
/// ends on is reported once through `onChange` and stays shown until `value`
/// changes.
struct SlimSlider: View {
    let value: Float
    let onDrag: (Float) -> Void
    let onChange: (Float) -> Void

    /// The level under the pointer, from the gesture's start until `value` changes.
    @State
    private var dragValue: Float?

    /// The track's thickness.
    private static let trackHeight: CGFloat = 5

    private static func level(at x: CGFloat, width: CGFloat) -> Float {
        Float(max(0, min(1, x / width)))
    }

    private var clamped: Float { max(0, min(1, dragValue ?? value)) }

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
                        dragValue = level
                        onChange(level)
                    },
            )
        }
        .frame(height: 20)
        // The level the gesture ended on stays shown until the stored level
        // changes, to the committed one or to anything else, so the slider
        // never shows the old level in between.
        .onChange(of: value) { dragValue = nil }
        .accessibilityElement(children: .ignore)
        .accessibilityValue(Text(Double(clamped).formatted(.percent)))
        .accessibilityAdjustableAction { direction in
            let step: Float = 0.05
            switch direction {
            case .increment: onChange(min(1, clamped + step))
            case .decrement: onChange(max(0, clamped - step))
            @unknown default: break
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

#endif
