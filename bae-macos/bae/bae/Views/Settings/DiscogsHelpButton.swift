import BaeKit
import SwiftUI

/// The ⓘ after Search Discogs: what Discogs is, how to connect it, and where
/// the key is kept.
struct DiscogsHelpButton: View {
    @State
    private var isShowing = false

    var body: some View {
        Button {
            isShowing = true
        } label: {
            Image(systemName: "info.circle")
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text("About Discogs"))
        .popover(isPresented: $isShowing, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text(
                    "[Discogs](https://www.discogs.com) is a music database with detailed release info. To connect, [get your free API key](https://www.discogs.com/settings/developers)."
                )
                Text("Your key is kept in this Mac’s keychain.")
            }
            .themeText(.body)
            .frame(width: 280, alignment: .leading)
            .padding(ThemeSpace.edge)
        }
    }
}

#if DEBUG
    #Preview("Discogs Help") {
        DiscogsHelpButton()
            .padding()
    }
#endif
