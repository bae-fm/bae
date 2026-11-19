import BaeKit
import SwiftUI

/// The album's Credits: who did what on the release and its tracks, a row for
/// each role and set of tracks core groups, the role over the artists.
struct MacAlbumCredits: View {
    let credits: [BridgeCreditRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            MacFormHeading(title: String(localized: "Credits"))
            MacFactsGrid(wide: true) {
                ForEach(Array(credits.enumerated()), id: \.offset) { _, row in
                    MacFact(verbatim: row.role.text) {
                        Text(verbatim: row.text)
                    }
                }
            }
            .padding(.vertical, ThemeSpace.related)
        }
    }
}
