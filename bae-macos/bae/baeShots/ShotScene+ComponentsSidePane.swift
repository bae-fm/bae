import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueuePanel` on its Playlists and History tabs, as
    /// `MacQueuePanelContainer` draws them from the stores: the playlists,
    /// one open, the day's plays, no plays, and no plays from Ben.
    static let sidePaneComponentScenes: [ShotScene] = [
        playlistsPanel(
            "mac.component.queue-panel.playlists",
            openPlaylist: nil
        ),
        playlistsPanel(
            "mac.component.queue-panel.playlist-open",
            openPlaylist: "playlist-late-night"
        ),
        historyPanel(
            "mac.component.queue-panel.history",
            plays: boardPlays
        ),
        historyPanel("mac.component.queue-panel.history-empty", plays: []),
        historyPanel(
            "mac.component.queue-panel.history-empty-for-a-person",
            plays: [],
            person: benMemberId
        ),
    ]

    /// The playlists the panel's preview lists: Ana's and one Ben shared.
    private static let panelPlaylists: [BridgePlaylistSummary] = [
        boardPlaylistListing[0],
        BridgePlaylistSummary(
            id: "playlist-sunday-mornings",
            name: "Sunday mornings",
            sharing: .household,
            owner: BridgePersonRef(
                person: "person-Ben",
                name: "Ben",
                initials: "B",
                avatarVersion: nil
            ),
            yours: false,
            trackCount: BridgeCountLabel(
                key: "core.library.track_count",
                count: 1
            ),
            totalDuration: .minutesOnly(minutes: 4),
            cover: .single(cover: cover("Copper Sky"))
        ),
    ]

    /// The panel on Playlists, listing the playlists, or with the playlist
    /// `openPlaylist` open, whose page the slot leaves empty.
    private static func playlistsPanel(
        _ id: String,
        openPlaylist: String?
    ) -> ShotScene {
        let uiStore = UiStore()
        uiStore.openSidePane(.playlists)
        if let openPlaylist {
            uiStore.openPlaylist(openPlaylist)
        }
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            playlists: panelPlaylists
        )
        let projections = LibraryProjectionStore(library: backing.library)
        return sidePanel(
            id,
            uiStore: uiStore,
            backing: backing,
            stores: (
                projections,
                HistoryStore(library: backing.library, onError: { _ in })
            )
        ) {
            openPlaylist == nil
                ? projections.playlists.delivered
                : projections.playlist.delivered
        }
    }

    /// The panel on History, showing `plays`, the plays of the person with
    /// member id `person` when one is picked.
    private static func historyPanel(
        _ id: String,
        plays: [BridgeHistoryPlay],
        person: String? = nil
    ) -> ShotScene {
        let uiStore = UiStore()
        uiStore.openSidePane(.history)
        uiStore.showHistory(of: person)
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            history: plays,
            historyPeople: [(anaMemberId, "Ana"), (benMemberId, "Ben")]
        )
        let history = HistoryStore(library: backing.library, onError: { _ in })
        return sidePanel(
            id,
            uiStore: uiStore,
            backing: backing,
            stores: (LibraryProjectionStore(library: backing.library), history)
        ) {
            history.list?.count != nil
        }
    }

    /// `MacQueuePanelContainer` in the preview's 600pt-tall frame, reading
    /// the stores a window passes it.
    private static func sidePanel(
        _ id: String,
        uiStore: UiStore,
        backing: (library: Library, session: LibraryBrowseSession),
        stores: (projections: LibraryProjectionStore, history: HistoryStore),
        isReady: @escaping @MainActor () -> Bool
    ) -> ShotScene {
        component(
            id,
            width: 436,
            height: 616,
            images: coverStore(),
            isReady: isReady
        ) {
            MacQueuePanelContainer(onClose: {}, onInsertTracks: { _, _ in })
                .environment(uiStore)
                .environment(backing.library)
                .environment(backing.session)
                .environment(stores.projections)
                .environment(stores.history)
                .frame(height: 600)
        }
    }
}
