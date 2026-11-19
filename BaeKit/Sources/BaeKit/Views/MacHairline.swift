import SwiftUI

/// The design system's hairline: a 1pt line in `Theme.hairline`, across a
/// pane or, with `axis` vertical, down its edge.
public struct MacHairline: View {
    private let axis: Axis

    public init(axis: Axis = .horizontal) {
        self.axis = axis
    }

    public var body: some View {
        Rectangle()
            .fill(Theme.hairline)
            .frame(
                width: axis == .vertical ? ThemeSpace.hairline : nil,
                height: axis == .horizontal ? ThemeSpace.hairline : nil
            )
    }
}
