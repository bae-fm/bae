import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueuePanel` on its Playlists and History tabs, as its preview
    /// draws them: the playlists, one open with its page left out, the
    /// first day's heading with the plays left out, no plays, and no plays
    /// from Ben.
    static let sidePaneComponentScenes: [ShotScene] = [
        sidePanel(
            "mac.component.queue-panel.playlists",
            tab: .playlists,
            lead: .newPlaylist {}
        ) {
            PlaylistList(listing: panelPlaylists)
        },
        sidePanel(
            "mac.component.queue-panel.playlist-open",
            tab: .playlists,
            lead: .back(to: String(localized: "Playlists")) {}
        ) {
            Color.clear
        },
        sidePanel("mac.component.queue-panel.history", tab: .history) {
            MacQueueSection(
                title: String(localized: "Today"),
                kind: .day(
                    menu: historyMenu(person: nil)
                        .inlineMenu(action: false) { _ in }
                )
            )
            // The margin the plays' times hang into, which the preview
            // leaves 40pt wide.
            .padding(
                .leading,
                ThemeSpace.related + MacListRow.hang(leadWidth: 40)
            )
            .padding(.trailing, ThemeSpace.related)
        },
        sidePanel(
            "mac.component.queue-panel.history-empty",
            tab: .history
        ) {
            HistoryEmpty(menu: historyMenu(person: nil)) { _ in }
        },
        sidePanel(
            "mac.component.queue-panel.history-empty-for-a-person",
            tab: .history
        ) {
            HistoryEmpty(menu: historyMenu(person: benMemberId)) { _ in }
        },
    ]

    /// The playlists the panel's preview lists: Ana's and one Ben shared.
    private static var panelPlaylists: BridgePlaylistListing {
        let playlists = [
            boardPlaylistListing[0],
            BridgePlaylistSummary(
                id: "playlist-sunday-mornings",
                name: "Sunday mornings",
                sharing: .household,
                owner: owner("Ben"),
                yours: false,
                trackCount: BridgeCountLabel(
                    key: "core.library.track_count",
                    count: 1
                ),
                totalDuration: .minutesOnly(minutes: 4),
                cover: .single(cover: cover("Copper Sky"))
            ),
        ]
        return playlistListing(playlists)
    }

    /// History's choice of whose plays, offering Ana and Ben, on the plays
    /// of the person with member id `person`, or everyone's for `nil`.
    private static func historyMenu(person: String?) -> HistoryMenu {
        HistoryMenu(
            count: BridgeListCount(
                rows: 0,
                items: 0,
                extra: boardHistoryMenu(
                    [(anaMemberId, "Ana"), (benMemberId, "Ben")],
                    person: person
                )
            )
        )
    }

    /// The panel on `tab`, starting its top with `lead`, holding `content`,
    /// in the preview's 600pt-tall frame.
    private static func sidePanel<Content: View>(
        _ id: String,
        tab: SidePaneTab,
        lead: SidePaneLead = .none,
        @ViewBuilder content: @escaping @MainActor () -> Content
    ) -> ShotScene {
        component(id, width: 436, height: 616, images: coverStore()) {
            // The tabs' content reads the library, the window's state and
            // the grid's order for what a click or a drop does.
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore)
            MacQueuePanel(
                tab: tab,
                onSelectTab: { _ in },
                lead: lead,
                onClose: {},
                content: { content() }
            )
            .frame(height: 600)
            .environment(backing.library)
            .environment(backing.session)
            .environment(uiStore)
        }
    }
}
