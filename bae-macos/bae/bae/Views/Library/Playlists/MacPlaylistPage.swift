import BaeKit
import SwiftUI

/// A playlist's page inside the side pane's Playlists tab: its picture beside
/// its name, owner and facts over Play and its ⋯ menu, then its tracks in
/// order.
struct MacPlaylistPage: View {
    /// The playlist as its read last delivered it; nil until the read of the
    /// open playlist delivers it.
    let detail: BridgePlaylistDetail?

    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue
    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var editing: EditPlaylistSheetState?
    @State
    private var deleting: BridgePlaylistSummary?

    private static var pictureSize: CGFloat { 103 }
    /// The width of the column the entries' numbers hang in, left of the
    /// picture's edge.
    static let numberWidth: CGFloat = 20

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if let detail {
                    header(detail.summary)
                    PlaylistEntryList(
                        detail: detail,
                        play: { entry in
                            playback.playPlaylist(
                                detail.summary.id,
                                entry.positionId,
                                false
                            )
                        },
                        playNext: { queue.addNext([$0.trackId]) },
                        addToQueue: { queue.addToQueue([$0.trackId]) },
                        remove: { removeEntry($0, from: detail.summary) },
                        move: { await move($0, to: $1, in: detail.summary) }
                    )
                }
                // The open playlist's read is local: nothing shows until it
                // lands.
            }
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.section)
            // The margin holds the entries' numbers, which hang left of the
            // picture's edge.
            .padding(
                .horizontal,
                ThemeSpace.related
                    + MacListRow.hang(leadWidth: Self.numberWidth)
            )
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollIndicators(.never)
        .sheet(item: $editing) { sheet in
            EditPlaylistSheet(
                // While the sheet closes, it shows the state it had before
                // `editing` became nil.
                state: Binding(
                    get: { editing ?? sheet },
                    set: { editing = $0 }
                ),
                onCancel: { editing = nil },
                onSave: { save($0) }
            )
        }
        .macAlert(item: $deleting) { playlist in
            DeletePlaylistConfirmation.alert(name: playlist.name) {
                delete(playlist)
            }
        }
    }

    /// The picture beside a column of the name, the owner, the count and
    /// length, and Play with the ⋯ menu, as an album's page lays its header
    /// out.
    private func header(_ summary: BridgePlaylistSummary) -> some View {
        HStack(alignment: .top, spacing: ThemeSpace.edge) {
            PlaylistPicture(cover: summary.cover, size: Self.pictureSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(summary.name)
                    .themeText(.title)
                    .lineLimit(2)
                HStack(spacing: ThemeSpace.compact) {
                    Avatar(
                        initials: summary.owner.initials,
                        picture: summary.owner.avatarVersion.map {
                            (summary.owner.person, $0)
                        },
                        size: 16
                    )
                    Text(verbatim: summary.owner.name)
                        .themeText(.strong)
                        .lineLimit(1)
                }
                .foregroundStyle(.secondary)
                facts(summary)
                    .padding(.top, ThemeSpace.inline)
                HStack(spacing: ThemeSpace.related) {
                    Button("Play") {
                        playback.playPlaylist(summary.id, nil, false)
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    MacMoreMenu(boxed: true, label: Text("Playlist options")) {
                        PlaylistOptionsMenu(
                            onShuffle: {
                                playback.playPlaylist(summary.id, nil, true)
                            },
                            owner: summary.yours
                                ? PlaylistOptionsMenu.OwnerActions(
                                    edit: {
                                        editing = EditPlaylistSheetState(
                                            summary
                                        )
                                    },
                                    delete: { deleting = summary }
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
    private func facts(_ summary: BridgePlaylistSummary) -> some View {
        HStack(spacing: ThemeSpace.compact) {
            Text(
                FactList.joined([
                    summary.trackCount.localizedLine,
                    summary.totalDuration?.text,
                ])
            )
            .themeText(.detail)
            .lineLimit(1)
            if summary.yours {
                Image(
                    systemName: summary.sharing == .household
                        ? "person.2.fill" : "lock.fill"
                )
                .font(.system(size: 10))
                .accessibilityLabel(Text(verbatim: summary.sharing.label))
            }
        }
        .foregroundStyle(.tertiary)
    }

    /// Save the name and who sees `sheet`'s playlist in one write; when the
    /// write fails, the sheet stays open and shows the error.
    private func save(_ sheet: EditPlaylistSheetState) {
        let library = library
        Task {
            do {
                try await library.playlistWrites.edit(
                    sheet.playlistId,
                    sheet.name,
                    sheet.sharing
                )
                editing = nil
            }
            catch {
                editing?.error = DisplayError(error)
            }
        }
    }

    /// Delete `playlist` and go back to the list when it is open. When the
    /// delete fails, show the error.
    private func delete(_ playlist: BridgePlaylistSummary) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                try await library.playlistWrites.delete(playlist.id)
                uiStore.closePlaylist(playlist.id)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func removeEntry(
        _ entry: BridgePlaylistEntryRow,
        from playlist: BridgePlaylistSummary
    ) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                try await library.playlistWrites.removeEntry(
                    playlist.id,
                    entry.positionId
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// Move the entry `positionId` to `place` in `playlist`, and show the
    /// error when the write fails.
    private func move(
        _ positionId: String,
        to place: BridgeEntryPlace,
        in playlist: BridgePlaylistSummary
    ) async {
        do {
            try await library.playlistWrites.moveEntry(
                playlist.id,
                positionId,
                place
            )
        }
        catch {
            uiStore.showError(error)
        }
    }
}
