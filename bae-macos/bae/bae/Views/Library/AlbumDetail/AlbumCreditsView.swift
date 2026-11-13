import BaeKit
import SwiftUI

/// The album's Credits: who did what on the release and its tracks, a row for
/// each role and set of tracks core groups, the role over the artists.
struct AlbumCreditsView: View {
    let credits: [BridgeCreditRow]

    /// The width of the role column.
    private static let roleWidth: CGFloat = 140

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            FormSectionHeader(title: String(localized: "Credits"))
            Grid(
                alignment: .topLeading,
                horizontalSpacing: ThemeSpace.group,
                verticalSpacing: ThemeSpace.inline
            ) {
                ForEach(Array(credits.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        Text(verbatim: row.role.text)
                            .themeText(.detail)
                            .foregroundStyle(.secondary)
                            .frame(
                                width: Self.roleWidth,
                                alignment: .leading
                            )
                        Text(verbatim: row.text)
                            .themeText(.body)
                    }
                }
            }
            .padding(.vertical, ThemeSpace.related)
        }
    }
}
