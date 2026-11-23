import BaeKit
import SwiftUI

/// A row of five stars or a read-only rating as its number and one filled star.
struct Stars: View {
    private enum Form {
        case row(value: Int, onRate: ((BridgeStars) -> Void)?)
        case number(BridgeStars)
    }

    private let form: Form
    private let size: CGFloat

    /// A row with `value` filled stars. When `onRate` is present, each star
    /// rates with that many stars.
    init(value: Int, size: CGFloat, onRate: ((BridgeStars) -> Void)? = nil) {
        self.form = .row(value: value, onRate: onRate)
        self.size = size
    }

    /// A read-only rating such as “4 ★”.
    init(rating: BridgeStars, size: CGFloat) {
        self.form = .number(rating)
        self.size = size
    }

    var body: some View {
        switch form {
        case .row(let value, let onRate):
            row(value: value, onRate: onRate)
        case .number(let rating):
            number(rating)
        }
    }

    private func number(_ rating: BridgeStars) -> some View {
        HStack(spacing: ThemeSpace.line) {
            Text(verbatim: rating.number.formatted())
                .foregroundStyle(Theme.macTextSecondary)
            star(filled: true)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(valueLabel(rating.number))
        .accessibilityAddTraits(.isImage)
    }

    private func row(
        value: Int,
        onRate: ((BridgeStars) -> Void)?
    ) -> some View {
        HStack(spacing: ThemeSpace.line) {
            ForEach(BridgeStars.allCases, id: \.self) { stars in
                if let onRate {
                    Button {
                        onRate(stars)
                    } label: {
                        star(filled: stars.number <= value)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(
                        String(localized: "Rate \(stars.number) of 5 stars")
                    )
                }
                else {
                    star(filled: stars.number <= value)
                }
            }
        }
        .accessibilityElement(children: onRate == nil ? .ignore : .contain)
        .accessibilityLabel(onRate == nil ? valueLabel(value) : "")
        .accessibilityAddTraits(onRate == nil ? .isImage : [])
    }

    private func valueLabel(_ value: Int) -> String {
        value == 0
            ? String(localized: "Not rated")
            : String(localized: "\(value) of 5 stars")
    }

    private func star(filled: Bool) -> some View {
        StarShape()
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
            // The star's whole frame is clickable.
            .contentShape(Rectangle())
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
