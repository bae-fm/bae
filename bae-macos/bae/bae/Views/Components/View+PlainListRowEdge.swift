import SwiftUI

extension View {
    /// Puts a row of a plain macOS `List` `edge` from the list's sides.
    /// A plain list insets its rows by about 8 points at each side before
    /// `listRowInsets` applies, so the row asks for the rest; SwiftUI has
    /// no API to read or remove that inset, and the 8 is measured from shots.
    func plainListRowEdge(
        _ edge: CGFloat,
        top: CGFloat = 0,
        bottom: CGFloat = 0
    )
        -> some View
    {
        let plainListRowInset: CGFloat = 8
        return listRowInsets(
            EdgeInsets(
                top: top,
                leading: edge - plainListRowInset,
                bottom: bottom,
                trailing: edge - plainListRowInset
            )
        )
    }
}
