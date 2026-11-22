import BaeKit
import SwiftUI

/// A playlist page's head in the side pane: the picture beside a column of the
/// name, the owner, the count and length, and Play with the ⋯ menu, as an
/// album's page lays its header out.
struct MacPlaylistHeader: View {
    let summary: BridgePlaylistSummary
    let onEdit: () -> Void
    let onDelete: () -> Void

    @Environment(Playback.self)
    private var playback

    private static var pictureSize: CGFloat { 103 }

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.edge) {
            PlaylistPicture(cover: summary.cover, size: Self.pictureSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(summary.name)
                    .themeText(.title)
                    .lineLimit(2)
                MacPersonTag(person: summary.owner, strong: true)
                facts
                    .padding(.top, ThemeSpace.inline)
                HStack(spacing: ThemeSpace.related) {
                    MacButton("Play", kind: .primary) {
                        playback.playPlaylist(summary.id, nil, false)
                    }
                    MacMoreMenu(look: .boxed, label: Text("Playlist options")) {
                        PlaylistOptionsMenu(
                            onShuffle: {
                                playback.playPlaylist(summary.id, nil, true)
                            },
                            owner: summary.yours
                                ? PlaylistOptionsMenu.OwnerActions(
                                    edit: onEdit,
                                    delete: onDelete
                                ) : nil
                        )
                    }
                }
                .padding(.top, ThemeSpace.line)
            }
        }
    }

    /// The count and length; on the person's own playlist, who sees it as a
    /// glyph after them. A playlist someone else made says nothing about
    /// sharing, since it is shared to be seen at all.
    private var facts: some View {
        HStack(spacing: ThemeSpace.compact) {
            Text(
                FactList.joined([
                    summary.trackCount.localizedLine,
                    summary.totalDuration?.text,
                ])
            )
            .themeText(.detail)
            .lineLimit(1)
            if let sharing = summary.headerSharing {
                Image(
                    systemName: sharing == .household
                        ? "person.2.fill" : "lock.fill"
                )
                .themeIcon(.small)
                .accessibilityLabel(Text(verbatim: sharing.label))
            }
        }
        .foregroundStyle(Theme.textTertiary)
    }
}
