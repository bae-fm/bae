import SwiftUI

/// Where a box of rows puts one of its rows: the space between the row's
/// edges and its content, and the row's least height.
public struct MacRowPlacement: Equatable, Sendable {
    public var insets: EdgeInsets
    public var minHeight: CGFloat

    public init(insets: EdgeInsets, minHeight: CGFloat) {
        self.insets = insets
        self.minHeight = minHeight
    }

    /// The placement when this row belongs to the one above it.
    public func joiningRowAbove() -> Self {
        var placement = self
        placement.insets.top = 0
        placement.minHeight = 0
        return placement
    }
}

extension EnvironmentValues {
    /// The placement a box gives a row that places itself
    /// (`placesItselfInRow`); nil outside a box.
    @Entry
    public var macRowPlacement: MacRowPlacement?

}

// Container values came with macOS 15, which the Mac app requires; BaeKit
// also builds for iOS 17, which has no box of rows to read them.
@available(macOS 15, iOS 18, *)
extension ContainerValues {
    /// Whether the row belongs to the one above it.
    @Entry
    public var joinsRowAbove = false

    /// Whether the row applies its box's placement itself, inside its own
    /// frame, rather than the box putting the space around it: a row that is
    /// a button presses from its edges, as a System Settings row does.
    @Entry
    public var placesItselfInRow = false
}

extension View {
    /// Joins this row to the one above it in a settings box.
    @ViewBuilder
    public func joiningRowAbove() -> some View {
        if #available(macOS 15, iOS 18, *) {
            containerValue(\.joinsRowAbove, true)
        }
        else {
            self
        }
    }

    /// This row's content laid out in `placement`: as wide as the box, with
    /// the placement's space round it, at least its least height.
    public func placedInRow(_ placement: MacRowPlacement) -> some View {
        frame(maxWidth: .infinity, alignment: .leading)
            .padding(placement.insets)
            .frame(minHeight: placement.minHeight)
    }
}

extension View {
    /// Marks this row as one that applies its box's placement itself
    /// (`placesItselfInRow`).
    func placesItselfInRow() -> some View {
        modifier(PlacesItselfInRow())
    }
}

private struct PlacesItselfInRow: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 15, iOS 18, *) {
            content.containerValue(\.placesItselfInRow, true)
        }
        else {
            content
        }
    }
}
