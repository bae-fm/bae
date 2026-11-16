import BaeKit
import SwiftUI

/// Five stars filled in the accent up to `value`, the rest drawn as outlines.
/// With `onRate` each star is a button that rates with that many stars.
struct Stars: View {
    /// How many stars are filled, zero to five.
    let value: Int
    let size: CGFloat
    var onRate: ((BridgeStars) -> Void)?

    var body: some View {
        HStack(spacing: ThemeSpace.line) {
            ForEach(BridgeStars.allCases, id: \.self) { stars in
                if let onRate {
                    Button {
                        onRate(stars)
                    } label: {
                        star(stars.number)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(
                        String(localized: "Rate \(stars.number) of 5 stars")
                    )
                }
                else {
                    star(stars.number)
                }
            }
        }
        .accessibilityElement(children: onRate == nil ? .ignore : .contain)
        .accessibilityLabel(onRate == nil ? valueLabel : "")
    }

    private var valueLabel: String {
        value == 0
            ? String(localized: "Not rated")
            : String(localized: "\(value) of 5 stars")
    }

    private func star(_ index: Int) -> some View {
        let filled = index <= value
        return StarShape()
            .fill(filled ? Theme.accent : Color.clear)
            .overlay {
                StarShape()
                    .stroke(
                        filled ? Theme.accent : Theme.hairlineStrong,
                        style: StrokeStyle(
                            lineWidth: size * 1.5 / 24,
                            lineJoin: .round
                        )
                    )
            }
            .frame(width: size, height: size)
    }
}

/// The design system's star, drawn on a 24 by 24 grid.
private struct StarShape: Shape {
    func path(in rect: CGRect) -> Path {
        let scale = rect.width / 24
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * scale, y: rect.minY + y * scale)
        }
        var path = Path()
        path.move(to: point(12, 2))
        path.addLine(to: point(14.9, 8.6))
        path.addLine(to: point(22, 9.2))
        path.addLine(to: point(16.6, 13.9))
        path.addLine(to: point(18.2, 20.9))
        path.addLine(to: point(12, 17.3))
        path.addLine(to: point(5.8, 20.9))
        path.addLine(to: point(7.4, 13.9))
        path.addLine(to: point(2, 9.2))
        path.addLine(to: point(9.1, 8.6))
        path.closeSubpath()
        return path
    }
}
