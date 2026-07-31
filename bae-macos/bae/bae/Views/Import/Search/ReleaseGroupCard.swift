import AppKit
import BaeKit
import SwiftUI

/// Header card for a release group: the album's cover, title, and artist on
/// the left; the source name (linking out to the group's editorial page) and
/// the pre-formatted "year span · N pressings" line on the right. The group's
/// pressing rows render beneath it.
struct ReleaseGroupCard: View {
    let group: ReleaseGroup

    var body: some View {
        HStack(spacing: 14) {
            ImageView(content: group.coverImageContent, pointSize: 60)
                .frame(width: 60, height: 60)
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .strokeBorder(.white.opacity(0.08), lineWidth: 1)
                )

            VStack(alignment: .leading, spacing: 2) {
                Text(group.title)
                    .font(.system(size: 17, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.tail)
                if let artist = group.artist {
                    Text(artist)
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 8)

            VStack(alignment: .trailing, spacing: 5) {
                sourceLink
                Text(group.metaLabel)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
            }
        }
        .padding(.vertical, 12)
        .padding(.horizontal, 14)
        .background(
            Theme.surfaceElevated,
            in: RoundedRectangle(cornerRadius: 12)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(.white.opacity(0.08), lineWidth: 1)
        )
    }

    @ViewBuilder
    private var sourceLink: some View {
        if let url = group.groupUrl {
            Button {
                NSWorkspace.shared.open(url)
            } label: {
                HStack(spacing: 5) {
                    Text(group.sourceLabel)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.secondary)
                    Image(systemName: "arrow.up.right.square")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            .buttonStyle(.plain)
            .help("Open release group on \(group.sourceLabel)")
        }
        else {
            Text(group.sourceLabel)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.secondary)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Release group card") {
        ReleaseGroupCard(group: PreviewData.searchGroupExact)
            .padding()
            .frame(width: 560)
            .importPreviewEnvironment()
    }
#endif
