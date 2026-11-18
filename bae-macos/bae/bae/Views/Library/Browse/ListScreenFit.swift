import BaeKit
import SwiftUI

/// What a list on screen measures to size its window: its height, and the
/// height of the shortest row it has drawn, placeholders included.
struct ListScreenFit: Equatable {
    var height: CGFloat = 0
    var rowHeight: CGFloat?

    /// The list drew a row `drawn` tall.
    mutating func drew(row drawn: CGFloat) {
        guard drawn > 0 else { return }
        rowHeight = min(rowHeight ?? drawn, drawn)
    }
}

extension View {
    /// Measure this list's height on screen into `fit`.
    func measuresScreen(into fit: Binding<ListScreenFit>) -> some View {
        onGeometryChange(for: CGFloat.self) {
            $0.size.height
        } action: {
            fit.wrappedValue.height = $0
        }
    }

    /// Measure this row's height into `fit`, which keeps the shortest.
    func measuresRow(into fit: Binding<ListScreenFit>) -> some View {
        onGeometryChange(for: CGFloat.self) {
            $0.size.height
        } action: {
            fit.wrappedValue.drew(row: $0)
        }
    }

    /// Size `list`'s window to the rows `fit` says the screen fits, each
    /// time the screen's height or its shortest row changes.
    func sizesWindow<Row: Identifiable & Sendable & Equatable>(
        of list: PaginatedList<Row>,
        from fit: ListScreenFit
    ) -> some View where Row.ID: Sendable {
        onChange(of: fit, initial: true) { _, measured in
            guard let rowHeight = measured.rowHeight else { return }
            list.fit(height: measured.height, rowHeight: rowHeight)
        }
    }
}
