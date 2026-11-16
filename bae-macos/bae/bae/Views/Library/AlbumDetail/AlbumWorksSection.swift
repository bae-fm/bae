import BaeKit
import SwiftUI

/// The album's Works: each work a track performs, after the track's number,
/// in the order of the tracks and then of the works on each. A work opens its
/// page.
struct AlbumWorksSection: View {
    let tracks: [Track]
    let onOpenWork: (String) -> Void

    /// The width of the track number column.
    private static let numberWidth: CGFloat = 20

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            FormSectionHeader(title: String(localized: "Works"))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                ForEach(tracks) { track in
                    ForEach(track.works, id: \.workId) { work in
                        HStack(spacing: ThemeSpace.related) {
                            Text(verbatim: track.positionText)
                                .themeText(.detail)
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                                .frame(
                                    width: Self.numberWidth,
                                    alignment: .leading
                                )
                            Button(work.title) { onOpenWork(work.workId) }
                                .buttonStyle(.accentPlain)
                            Text(
                                verbatim: FactList.glyph + " "
                                    + (work.composerNames ?? "")
                            )
                            .foregroundStyle(.secondary)
                            .opacity(work.composerNames == nil ? 0 : 1)
                            .accessibilityHidden(work.composerNames == nil)
                        }
                        .themeText(.body)
                        .lineLimit(1)
                    }
                }
            }
            .padding(.vertical, ThemeSpace.related)
        }
    }
}
