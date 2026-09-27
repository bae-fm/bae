import BaeKit
import SwiftUI

/// A knobless volume slider; it keeps no copy of `value` and reports clicks
/// and drags through `onChange`.
struct SlimSlider: View {
    let value: Float
    let onChange: (Float) -> Void

    private var clamped: Float { max(0, min(1, value)) }

    var body: some View {
        GeometryReader { geo in
            let width = geo.size.width
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Theme.hairline)
                    .frame(height: 5)
                Capsule()
                    .fill(Theme.accent)
                    .frame(width: CGFloat(clamped) * width, height: 5)
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
                        onChange(
                            Float(max(0, min(1, drag.location.x / width)))
                        )
                    },
            )
        }
        .frame(height: 20)
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

    /// Holds the value the way the now-playing bar does.
    private struct SlimSliderPreview: View {
        @State
        var value: Float

        var body: some View {
            SlimSlider(value: value, onChange: { value = $0 })
                .frame(width: 120)
                .padding()
                .background(Theme.surface)
        }
    }

    #Preview("Volume") {
        SlimSliderPreview(value: 0.6)
    }
#endif
