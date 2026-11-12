import BaeKit
import SwiftUI

/// An ⓘ after a row's title that opens a popover explaining the row when it
/// is clicked, as macOS Settings does beside a setting that needs explaining.
/// (`InfoTip` is the "?" that shows its text on hover.)
struct MacInfoTip<Content: View>: View {
    let label: LocalizedStringKey
    @ViewBuilder
    let content: Content

    @State
    private var isShowing = false

    var body: some View {
        Button {
            isShowing = true
        } label: {
            Image(systemName: "info.circle")
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.accentPlain)
        .accessibilityLabel(Text(label))
        .macPopover(isPresented: $isShowing, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                content
            }
            .themeText(.body)
            .foregroundStyle(.secondary)
            .frame(width: 280, alignment: .leading)
            .padding(ThemeSpace.edge)
        }
    }
}

/// What Search MusicBrainz's ⓘ says.
struct MusicBrainzInfoTip: View {
    var body: some View {
        MacInfoTip(label: "About MusicBrainz") {
            Text(
                "[MusicBrainz](https://musicbrainz.org) is an open music database that anyone can add to and edit."
            )
            Text("It needs no account or key.")
        }
    }
}

/// What Search Discogs's ⓘ says.
struct DiscogsInfoTip: View {
    var body: some View {
        MacInfoTip(label: "About Discogs") {
            Text(
                "[Discogs](https://www.discogs.com) is a music database with detailed release info."
            )
            VStack(alignment: .leading, spacing: 0) {
                Text(
                    "To connect, [get your free API key](https://www.discogs.com/settings/developers)."
                )
                Text("Your key is kept in this Mac’s keychain.")
            }
        }
    }
}

#if DEBUG
    #Preview("Info popovers") {
        HStack {
            MusicBrainzInfoTip()
            DiscogsInfoTip()
        }
        .padding()
    }
#endif
