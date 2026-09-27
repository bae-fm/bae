import AppKit
import BaeKit
import SwiftUI

/// A release group header: cover, title, artist and label, a note for each
/// source whose album links couldn't be read, and one link per source.
struct ReleaseGroupCard: View {
    let group: ReleaseGroup

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(
                content: group.coverImageContent,
                pointSize: ThemeSize.rowArtwork
            )
            .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            .overlay(
                RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    .strokeBorder(Theme.hairline, lineWidth: 1)
            )

            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                Text(group.title)
                    .themeText(.heading)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if !attribution.isEmpty {
                    Text(attribution)
                        .themeText(.body)
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
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                    }
                }
            }

            Spacer(minLength: ThemeSpace.related)

            HStack(spacing: ThemeSpace.related) {
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
                HStack(spacing: ThemeSpace.inline) {
                    Text(name)
                    Image(systemName: "arrow.up.right")
                        .themeIcon(.small)
                        .foregroundStyle(.tertiary)
                }
                .themeText(.chip)
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help(String(localized: "Open this album on \(name)"))
        }
        else {
            Text(name)
                .themeText(.chip)
                .foregroundStyle(.secondary)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Release group card") {
        VStack(alignment: .leading, spacing: ThemeSpace.edge) {
            ReleaseGroupCard(group: PreviewData.searchGroupExact)
            ReleaseGroupCard(group: PreviewData.searchGroupsManual[1])
            ReleaseGroupCard(group: PreviewData.searchGroupLinksUnread)
        }
        .padding()
        .frame(width: 560)
        .importPreviewEnvironment()
    }
#endif
