import BaeKit
import SwiftUI

/// A row's trailing mark for an import waiting for the worker, which has no
/// progress to draw yet.
struct QueuedImportIcon: View {
    /// The square the board draws the clock in.
    static let box: CGFloat = 12

    var body: some View {
        let waiting = String(localized: "Queued to import")
        Image(systemName: "clock")
            .themeIcon(.small)
            .frame(width: Self.box, height: Self.box)
            .foregroundStyle(Theme.macTextSecondary)
            .help(waiting)
            .accessibilityLabel(waiting)
    }
}
