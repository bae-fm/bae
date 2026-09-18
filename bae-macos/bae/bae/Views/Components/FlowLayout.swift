import SwiftUI

/// Wraps its subviews onto as many rows as they need, like text — the chip
/// field, the add-token row and the records row all overflow a single line.
struct FlowLayout: Layout {
    /// The gap between two items on one row.
    var spacing: CGFloat = 5
    /// The gap between two rows. The same as `spacing` unless the caller says
    /// otherwise: a row of links sits closer to the next row than its links
    /// sit to each other.
    var rowSpacing: CGFloat?

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) -> CGSize {
        let width = proposal.width ?? .infinity
        return arrange(subviews, in: width).size
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) {
        let arrangement = arrange(subviews, in: bounds.width)
        for (subview, position) in zip(subviews, arrangement.positions) {
            subview.place(
                at: CGPoint(
                    x: bounds.minX + position.x,
                    y: bounds.minY + position.y
                ),
                proposal: .unspecified
            )
        }
    }

    /// Row-wrap the subviews at their ideal sizes; positions are top-aligned
    /// within each row.
    private func arrange(
        _ subviews: Subviews,
        in width: CGFloat
    ) -> (size: CGSize, positions: [CGPoint]) {
        let rowSpacing = rowSpacing ?? spacing
        var positions: [CGPoint] = []
        var rowStart = 0
        var cursorX: CGFloat = 0
        var cursorY: CGFloat = 0
        var rowHeight: CGFloat = 0
        var maxX: CGFloat = 0

        func closeRow(endingBefore index: Int) {
            // Vertically center the row's items within the row height.
            for i in rowStart..<index {
                let size = subviews[i].sizeThatFits(.unspecified)
                positions[i].y += (rowHeight - size.height) / 2
            }
        }

        for (index, subview) in subviews.enumerated() {
            let size = subview.sizeThatFits(.unspecified)
            if cursorX > 0, cursorX + size.width > width {
                closeRow(endingBefore: index)
                cursorX = 0
                cursorY += rowHeight + rowSpacing
                rowHeight = 0
                rowStart = index
            }
            positions.append(CGPoint(x: cursorX, y: cursorY))
            cursorX += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            maxX = max(maxX, cursorX - spacing)
        }
        closeRow(endingBefore: subviews.count)

        return (CGSize(width: maxX, height: cursorY + rowHeight), positions)
    }
}
