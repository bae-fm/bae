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
    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var editing: EditPlaylistSheetState?
    @State
    private var deleting: BridgePlaylistSummary?

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
        .sheet(item: $editing) { sheet in
            EditPlaylistSheet(
                // The dismissal frame shows the last real value.
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
                        ]) + " " + FactList.glyph
                    )
                    StatusChip(verbatim: summary.sharing.label)
                }
                .themeText(.detail)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: ThemeSpace.related)
            Button("Play") { playback.playPlaylist(summary.id, nil, false) }
                .buttonStyle(PrimaryButtonStyle())
            Button("Shuffle") { playback.playPlaylist(summary.id, nil, true) }
            if summary.yours {
                MacMoreMenu {
                    MacMenuItem("Edit playlist…") {
                        editing = EditPlaylistSheetState(summary)
                    }
                    MacMenuDivider()
                    MacMenuItem("Delete playlist…") { deleting = summary }
                }
            }
        }
    }

    /// Save the name and who sees `sheet`'s playlist, writing only what
    /// changed; a failure stays in the sheet, which stays open, and saving
    /// again writes what is still different.
    private func save(_ sheet: EditPlaylistSheetState) {
        guard let current = detail?.summary, current.id == sheet.playlistId
        else {
            editing = nil
            return
        }
        let library = library
        Task {
            do {
                if sheet.name != current.name {
                    try await library.playlistWrites.rename(
                        current.id,
                        sheet.name
                    )
                }
                if sheet.sharing != current.sharing {
                    try await library.playlistWrites.setSharing(
                        current.id,
                        sheet.sharing
                    )
                }
                editing = nil
            }
            catch {
                editing?.error = DisplayError(error)
            }
        }
    }

    /// Delete `playlist`; its page shows the Playlists placeholder once the
    /// list drops it. A failure says why.
    private func delete(_ playlist: BridgePlaylistSummary) {
        let library = library
        let uiStore = uiStore
        let session = session
        Task {
            do {
                try await library.playlistWrites.delete(playlist.id)
                if session.selectedPlaylistId == playlist.id {
                    session.selectPlaylist(nil)
                }
            }
            catch {
                uiStore.showError(error)
            }
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
