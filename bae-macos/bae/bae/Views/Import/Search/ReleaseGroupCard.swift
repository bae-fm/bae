import AppKit
import BaeKit
import SwiftUI

/// Header for a release group: the album's cover, its title and the artist and
/// label beneath it, and on the right one outbound link per source carrying
/// it. A source whose page linking the album to the other catalog could not be
/// read says so under the title, since the album may then be listed twice.
/// The group's pressing rows render beneath.
struct ReleaseGroupCard: View {
    let group: ReleaseGroup

    var body: some View {
        HStack(spacing: 12) {
            ImageView(content: group.coverImageContent, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .strokeBorder(Theme.hairline, lineWidth: 1)
                )

            VStack(alignment: .leading, spacing: 1) {
                Text(group.title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.tail)
                if !attribution.isEmpty {
                    Text(attribution)
                        .font(.system(size: 12.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                ForEach(Array(group.sources.enumerated()), id: \.offset) {
                    _,
                    source in
                    if source.albumLinksUnread {
                        let name = bridgeCatalogName(catalog: source.source)
                        Text(
                            "Couldn't read this album's links on \(name); it may also be listed separately."
                        )
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    }
                }
            }

            Spacer(minLength: 8)

            HStack(spacing: 10) {
                // A card can carry two albums of one catalog, so a source is
                // told apart by its place rather than its catalog.
                ForEach(Array(group.sources.enumerated()), id: \.offset) {
                    _,
                    source in
                    AlbumSourceLink(source: source)
                }
            }
        }
    }

    /// Who made the album and who put it out — the two facts core names for
    /// the card, joined only where both are there.
    private var attribution: String {
        [group.artist, group.label]
            .compactMap { $0 }
            .joined(separator: " \u{00b7} ")
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
                HStack(spacing: 4) {
                    Text(name)
                    Image(systemName: "arrow.up.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(.tertiary)
                }
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help(String(localized: "Open this album on \(name)"))
        }
        else {
            Text(name)
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(.secondary)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Release group card") {
        VStack(alignment: .leading, spacing: 18) {
            ReleaseGroupCard(group: PreviewData.searchGroupExact)
            ReleaseGroupCard(group: PreviewData.searchGroupsManual[1])
            ReleaseGroupCard(group: PreviewData.searchGroupLinksUnread)
        }
        .padding()
        .frame(width: 560)
        .importPreviewEnvironment()
    }
#endif
