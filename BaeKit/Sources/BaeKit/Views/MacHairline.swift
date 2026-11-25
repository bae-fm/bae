import SwiftUI

/// The design system's hairline: a 1pt line in `Theme.hairline`, across a
/// pane or, with `axis` vertical, down its edge. `faint` draws it at 60
/// percent, for a divider between rows inside a card.
public struct MacHairline: View {
    private let axis: Axis
    private let faint: Bool

    public init(axis: Axis = .horizontal, faint: Bool = false) {
        self.axis = axis
        self.faint = faint
    }

    public var body: some View {
        Rectangle()
            .fill(faint ? Theme.hairline.opacity(0.6) : Theme.hairline)
            .frame(
                width: axis == .vertical ? ThemeSpace.hairline : nil,
                height: axis == .horizontal ? ThemeSpace.hairline : nil
            )
    }
}
