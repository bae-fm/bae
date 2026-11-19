import BaeKit
import SwiftUI

/// A row's trailing mark for an import waiting for the worker, which has no
/// progress to draw yet.
struct QueuedImportIcon: View {
    var body: some View {
        let waiting = String(localized: "Queued to import")
        Image(systemName: "clock")
            .themeIcon(.small)
            .foregroundStyle(.secondary)
            .help(waiting)
            .accessibilityLabel(waiting)
    }
}
