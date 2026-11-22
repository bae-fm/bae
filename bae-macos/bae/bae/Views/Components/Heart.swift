import SwiftUI

/// A rating in a library whose rating system is Heart, read-only: one heart,
/// filled in the accent when `on` (the person gave three stars or more), an
/// outline otherwise.
struct Heart: View {
    var on = false
    let size: CGFloat

    var body: some View {
        HeartShape()
            .fill(on ? Theme.accent : Color.clear)
            .overlay {
                HeartShape()
                    .stroke(
                        on ? Theme.accent : Theme.hairlineStrong,
                        style: StrokeStyle(
                            lineWidth: size * 1.5 / 24,
                            lineJoin: .round
                        )
                    )
            }
            .frame(width: size, height: size)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(String(localized: "Hearted"))
            .accessibilityHidden(!on)
    }
}

/// The design system's heart, drawn on a 24 by 24 grid.
private struct HeartShape: Shape {
    func path(in rect: CGRect) -> Path {
        let scale = rect.width / 24
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * scale, y: rect.minY + y * scale)
        }
        // M12 20.5 s-7.6-4.6-9.5-9.3 C1.1 7.8 3.3 4.3 6.8 4.3
        // c2 0 3.6 1 5.2 2.9 1.6-1.9 3.2-2.9 5.2-2.9 3.5 0 5.7 3.5 4.3 6.9
        // -1.9 4.7-9.5 9.3-9.5 9.3z
        var path = Path()
        path.move(to: point(12, 20.5))
        path.addCurve(
            to: point(2.5, 11.2),
            control1: point(12, 20.5),
            control2: point(4.4, 15.9)
        )
        path.addCurve(
            to: point(6.8, 4.3),
            control1: point(1.1, 7.8),
            control2: point(3.3, 4.3)
        )
        path.addCurve(
            to: point(12, 7.2),
            control1: point(8.8, 4.3),
            control2: point(10.4, 5.3)
        )
        path.addCurve(
            to: point(17.2, 4.3),
            control1: point(13.6, 5.3),
            control2: point(15.2, 4.3)
        )
        path.addCurve(
            to: point(21.5, 11.2),
            control1: point(20.7, 4.3),
            control2: point(22.9, 7.8)
        )
        path.addCurve(
            to: point(12, 20.5),
            control1: point(19.6, 15.9),
            control2: point(12, 20.5)
        )
        path.closeSubpath()
        return path
    }
}
