import SwiftUI

/// A slim, knobless slider for the now-playing bar's volume: a 5pt capsule
/// track inside a taller hit area. Click and drag both set the value in 0…1
/// through `onChange`. `value` is store-driven — the parent passes the rendered
/// volume and re-renders when it changes, so the control keeps no copy.
struct SlimSlider: View {
    let value: Float
    let onChange: (Float) -> Void

    private var clamped: Float { max(0, min(1, value)) }

    var body: some View {
        GeometryReader { geo in
            let width = geo.size.width
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.white.opacity(0.10))
                    .frame(height: 5)
                Capsule()
                    .fill(Color.white.opacity(0.75))
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
