import BaeKit
import SwiftUI

/// The notice that Discogs is not among the sources Find online can ask,
/// shown above both sections because it holds for each of them.
struct FindOnlineDiscogsBar: View {
    /// Open settings on the Discogs page.
    let onOpenSettings: () -> Void
    /// Put the bar away for the rest of this run of the app.
    let onDismiss: () -> Void

    private var discogs: String {
        bridgeCatalogName(catalog: .discogs)
    }

    var body: some View {
        Notice(
            title: String(localized: "Add \(discogs) to find more pressings"),
            advice: String(
                localized:
                    "Many regional releases, reissues and vinyl pressings are only on \(discogs). Tokens are free: create one in your \(discogs) account settings."
            ),
            error: nil,
            tone: .info,
            action: ErrorAction("Open settings", perform: onOpenSettings),
            onDismiss: onDismiss
        )
        .padding(ThemeSpace.group)
    }
}

#if DEBUG
    // MARK: - Previews

#endif
