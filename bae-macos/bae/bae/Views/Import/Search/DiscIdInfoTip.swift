import BaeKit
import SwiftUI

/// The shared "what is DiscID" info tip shown beside the identify banners and the
/// conflict surface — explains that disc-id lookup uses track layout to find
/// exact MusicBrainz matches.
struct DiscIdInfoTip: View {
    var body: some View {
        InfoTip(
            text: "Uses track layout to find exact matches on MusicBrainz.",
            learnMoreURL: URL(
                string: "https://bae.fm/importing/local-files#identify"
            ),
            width: 260,
        )
    }
}

#if DEBUG
    #Preview("DiscID info tip") {
        HStack {
            Text("No automatic matches found")
                .font(.callout)
            DiscIdInfoTip()
        }
        .padding()
        .windowBackground()
    }
#endif
