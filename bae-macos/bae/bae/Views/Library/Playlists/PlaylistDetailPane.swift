import BaeKit
import SwiftUI

/// A playlist's page: its picture, name and owner over Play, then its tracks
/// in order.
struct PlaylistDetailPane: View {
    let detail: BridgePlaylistDetail?

    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(Playback.self)
    private var playback

    private static var pictureSize: CGFloat { 72 }

    var body: some View {
        if detail == nil, session.selectedPlaylistId == nil {
            // Nothing chosen: the placeholder fills the pane, centred.
            PlaceholderTitle("Playlists")
                .background(Theme.surface)
        }
        else {
            page
        }
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if let detail {
                    header(detail.summary)
                    entries(detail)
                }
                // A selected playlist's read is local: nothing shows until it
                // lands.
            }
            .padding(ThemeSpace.section)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "playlistDetail")
        .background(Theme.surface)
    }

    private func header(_ summary: BridgePlaylistSummary) -> some View {
        HStack(spacing: ThemeSpace.edge) {
            PlaylistPicture(cover: summary.cover, size: Self.pictureSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(summary.name)
                    .themeText(.title)
                    .lineLimit(2)
                HStack(spacing: ThemeSpace.inline) {
                    Avatar(
                        name: summary.owner.name,
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
                        ])
                    )
                    Text(summary.sharing.label)
                        .padding(.horizontal, ThemeSpace.related)
                        .padding(.vertical, ThemeSpace.hairline)
                        .background(Theme.well, in: Capsule())
                }
                .themeText(.detail)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: ThemeSpace.related)
            Button("Play") { playback.playPlaylist(summary.id, nil, false) }
                .buttonStyle(PrimaryButtonStyle())
            Button("Shuffle") { playback.playPlaylist(summary.id, nil, true) }
        }
    }

    private func entries(_ detail: BridgePlaylistDetail) -> some View {
        LazyVStack(spacing: 0) {
            ForEach(
                Array(detail.entries.enumerated()),
                id: \.element.positionId
            ) { index, entry in
                PlaylistEntryRow(
                    number: index + 1,
                    entry: entry,
                    play: {
                        playback.playPlaylist(
                            detail.summary.id,
                            entry.positionId,
                            false
                        )
                    }
                )
            }
        }
    }
}

/// One entry of a playlist: double-clicking plays the playlist from it.
private struct PlaylistEntryRow: View {
    let number: Int
    let entry: BridgePlaylistEntryRow
    let play: () -> Void

    @State
    private var hovering = false

    var body: some View {
        MacListRow(
            lead: String(number),
            cover: entry.cover,
            title: entry.title,
            subtitle: FactList.joined([entry.artistLine.text, entry.albumTitle]
            ),
            duration: DurationClock.label(entry.durationClock),
            hover: hovering
        )
        .onHover { hovering = $0 }
        .onTapGesture(count: 2, perform: play)
    }
}
