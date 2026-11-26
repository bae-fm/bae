import AppKit
import BaeKit
import SwiftUI

/// A release group's header on the pressing table's grid: its cover, then its
/// title over its artist where the label column starts, and a rule under
/// them.
struct MacReleaseCard: View {
    private static let coverSize: CGFloat = 40

    let group: ReleaseGroup

    @Environment(\.pressingTable)
    private var table

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            HStack(spacing: ThemeSpace.edge) {
                Artwork(
                    content: group.coverImageContent,
                    pointSize: Self.coverSize,
                    cornerRadius: nil
                )
                .frame(width: Self.coverSize, height: Self.coverSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                // As wide as the year's column, so the title starts at the
                // label's.
                .frame(
                    width: table.width(of: .year),
                    alignment: .leading
                )

                VStack(alignment: .leading, spacing: 0) {
                    Text(group.title)
                        .themeText(.strong)
                        .cutLine(.prose)
                    if let artist = group.artist {
                        Text(artist)
                            .themeText(.body)
                            .foregroundStyle(Theme.textSecondary)
                            .cutLine(.prose)
                    }
                    ForEach(Array(group.sources.enumerated()), id: \.offset) {
                        _,
                        source in
                        if source.albumLinksUnread {
                            let name = bridgeCatalogName(catalog: source.source)
                            Text(
                                "Could not read this album’s links on \(name); it may also be listed separately."
                            )
                            .themeText(.detail)
                            .foregroundStyle(Theme.textSecondary)
                        }
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, ThemeSpace.related)
            MacHairline()
                .padding(.horizontal, ThemeSpace.related)
                .padding(.bottom, ThemeSpace.line)
        }
    }
}

/// One source's name, opening its editorial page for the album. A source that
/// returned the release ungrouped has no page, so its name is text.
struct AlbumSourceLink: View {
    let source: BridgeReleaseGroupSource

    var body: some View {
        let name = bridgeCatalogName(catalog: source.source)
        if let url = source.groupUrl.flatMap(URL.init(string:)) {
            Button {
                NSWorkspace.shared.open(url)
            } label: {
                HStack(spacing: ThemeSpace.inline) {
                    Text(name)
                    Image(systemName: "arrow.up.right")
                        .themeIcon(.small)
                        .foregroundStyle(Theme.textTertiary)
                }
                .themeText(.chip)
                .foregroundStyle(Theme.textSecondary)
            }
            .buttonStyle(.accentPlain)
            .help(String(localized: "Open this album on \(name)"))
        }
        else {
            Text(name)
                .themeText(.chip)
                .foregroundStyle(Theme.textSecondary)
        }
    }
}
