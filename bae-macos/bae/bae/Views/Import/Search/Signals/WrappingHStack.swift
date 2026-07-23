import SwiftUI

/// A horizontal layout that wraps its subviews onto new lines when they don't
/// fit the proposed width — like flex-wrap. The badge row uses it so badges
/// stay whole units and the row flows onto multiple lines at narrow widths.
struct WrappingHStack: Layout {
    var spacing: CGFloat = 8
    var lineSpacing: CGFloat = 8

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) -> CGSize {
        let maxWidth = proposal.width ?? .infinity
        let rows = layoutRows(subviews: subviews, maxWidth: maxWidth)
        let width = rows.map(\.width).max() ?? 0
        let height =
            rows.map(\.height).reduce(0, +)
            + lineSpacing * CGFloat(max(0, rows.count - 1))
        return CGSize(width: width, height: height)
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) {
        let rows = layoutRows(subviews: subviews, maxWidth: bounds.width)
        var y = bounds.minY
        for row in rows {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index]
                    .place(
                        at: CGPoint(x: x, y: y),
                        anchor: .topLeading,
                        proposal: ProposedViewSize(size)
                    )
                x += size.width + spacing
            }
            y += row.height + lineSpacing
        }
    }

    /// One wrapped line: the subview indices on it plus its measured extent.
    private struct Row {
        var indices: [Int] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func layoutRows(subviews: Subviews, maxWidth: CGFloat) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let projected =
                row.indices.isEmpty
                ? size.width : row.width + spacing + size.width
            if !row.indices.isEmpty, projected > maxWidth {
                rows.append(row)
                row = Row()
            }
            if row.indices.isEmpty {
                row.width = size.width
            }
            else {
                row.width += spacing + size.width
            }
            row.height = max(row.height, size.height)
            row.indices.append(index)
        }
        if !row.indices.isEmpty {
            rows.append(row)
        }
        return rows
    }
}
