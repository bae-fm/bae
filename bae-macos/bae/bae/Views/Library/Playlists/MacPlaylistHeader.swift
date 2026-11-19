import BaeKit
import SwiftUI

/// A playlist page's head: the picture, the name over the owner and the
/// facts with the sharing chip, then Play, Shuffle and, for the person's own
/// playlist, the options menu.
struct MacPlaylistHeader: View {
    let summary: BridgePlaylistSummary
    let onEdit: () -> Void
    let onDelete: () -> Void

    @Environment(Playback.self)
    private var playback

    private static var pictureSize: CGFloat { 72 }

    var body: some View {
        HStack(spacing: ThemeSpace.edge) {
            PlaylistPicture(cover: summary.cover, size: Self.pictureSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(summary.name)
                    .themeText(.title)
                    .lineLimit(2)
                HStack(spacing: ThemeSpace.inline) {
                    Avatar(
                        initials: summary.owner.initials,
                        picture: summary.owner.avatarVersion.map {
                            (summary.owner.person, $0)
                        },
                        size: 16
                    )
                    Text(
                        FactList.joined([
                            summary.owner.name,
                            summary.trackCount.localizedLine,
                            summary.totalDuration?.text,
                        ]) + " " + FactList.glyph
                    )
                    StatusChip(verbatim: summary.sharing.label)
                }
                .themeText(.detail)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: ThemeSpace.related)
            MacButton("Play", kind: .primary) {
                playback.playPlaylist(summary.id, nil, false)
            }
            Button("Shuffle") { playback.playPlaylist(summary.id, nil, true) }
            if summary.yours {
                MacMoreMenu {
                    PlaylistOptionsMenu(onEdit: onEdit, onDelete: onDelete)
                }
            }
        }
    }
}
