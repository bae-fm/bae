import AppKit
import BaeKit
import SwiftUI

/// Header for a release group: the album's cover, its title and the artist and
/// label beneath it, and on the right one outbound link per source carrying
/// it. The group's pressing rows render beneath.
struct ReleaseGroupCard: View {
    let group: ReleaseGroup

    var body: some View {
        HStack(spacing: 12) {
            ImageView(content: group.coverImageContent, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .strokeBorder(.white.opacity(0.08), lineWidth: 1)
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
            }

            Spacer(minLength: 8)

            HStack(spacing: 10) {
                ForEach(group.sources, id: \.source) { source in
                    sourceLink(source)
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

    /// One source's name, opening its editorial page for the album. A source
    /// that returned the release ungrouped has no page, so its name is text.
    @ViewBuilder
    private func sourceLink(_ source: BridgeReleaseGroupSource) -> some View {
        let name = bridgeMetadataSourceName(source: source.source)
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
        }
        .padding()
        .frame(width: 560)
        .importPreviewEnvironment()
    }
#endif
