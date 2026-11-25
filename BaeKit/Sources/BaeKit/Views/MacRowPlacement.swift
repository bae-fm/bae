import SwiftUI

/// How a row in a settings box belongs to the row above it.
public enum MacRowJoin: Sendable {
    /// A row of its own.
    case none
    /// A block, such as `MacNested`, that the row above holds: the blocks are
    /// 8 apart and the last has 12 below.
    case block
    /// A `MacRow` that is nested: it keeps a row's own space below it.
    case row
}

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
    /// How the row belongs to the one above it, if it does.
    @Entry
    public var rowJoin = MacRowJoin.none

    /// Whether the row applies its box's placement itself, inside its own
    /// frame, rather than the box putting the space around it: a row that is
    /// a button presses from its edges, as a System Settings row does.
    @Entry
    public var placesItselfInRow = false

    /// The space a form puts above this footer note, where it differs from
    /// the form's own; nil takes the form's.
    @Entry
    public var footerNoteSpace: CGFloat?
}

extension View {
    /// Joins this row to the one above it in a settings box.
    @ViewBuilder
    public func joiningRowAbove() -> some View {
        if #available(macOS 15, iOS 18, *) {
            containerValue(\.rowJoin, .block)
        }
        else {
            self
        }
    }

    /// Joins this row to the one above it as a `MacRow` does: the row's own
    /// space stays below it.
    @ViewBuilder
    func joiningRowAboveAsRow() -> some View {
        if #available(macOS 15, iOS 18, *) {
            containerValue(\.rowJoin, .row)
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
    /// Sets the space a form puts above this footer note, in place of the
    /// form's own.
    @ViewBuilder
    public func footerNoteSpace(_ space: CGFloat) -> some View {
        if #available(macOS 15, iOS 18, *) {
            containerValue(\.footerNoteSpace, space)
        }
        else {
            self
        }
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
