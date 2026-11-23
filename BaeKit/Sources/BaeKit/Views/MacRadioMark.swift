import SwiftUI

/// A radio's circle as the board draws it: 14 points, a ring in the faint
/// text colour, or while chosen a disc in the chosen accent with a 5-point
/// dot in the colour made for that fill.
public struct MacRadioMark: View {
    private let checked: Bool

    @Environment(\.accentChoice)
    private var accent

    private static let side: CGFloat = 14
    private static let dot: CGFloat = 5
    private static let ringWidth: CGFloat = 1.5

    public init(checked: Bool) {
        self.checked = checked
    }

    public var body: some View {
        Group {
            if checked {
                Circle()
                    .fill(accent.colors.fill)
                    .overlay {
                        Circle()
                            .fill(accent.colors.onFill)
                            .frame(width: Self.dot, height: Self.dot)
                    }
            }
            else {
                Circle()
                    .strokeBorder(
                        Theme.textTertiary,
                        lineWidth: Self.ringWidth
                    )
            }
        }
        .frame(width: Self.side, height: Self.side)
    }
}
