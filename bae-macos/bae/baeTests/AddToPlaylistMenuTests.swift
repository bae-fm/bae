import BaeKit
import Testing

@testable import bae

@Suite("Add to playlist menu")
struct AddToPlaylistMenuTests {
    private func summary(_ name: String) -> BridgePlaylistSummary {
        BridgePlaylistSummary(
            id: "id-\(name)",
            name: name,
            sharing: .private,
            owner: BridgePlaylistOwner(
                person: "person",
                name: "Person",
                avatarVersion: nil
            ),
            yours: true,
            trackCount: BridgeCountLabel(
                key: "core.library.track_count",
                count: 1
            ),
            totalDuration: nil,
            cover: .single(cover: nil)
        )
    }

    private func titles(_ entries: [MacMenuEntry]) -> [String] {
        entries.map {
            switch $0 {
            case .item(let item): item.title
            case .submenu(let title, _, _): title
            case .divider: "-"
            case .header(let title): title
            }
        }
    }

    @Test(
        "it offers nothing until the playlists are read, or without a profile"
    )
    func offersNothingWithoutPlaylistsOrAProfile() {
        let unread = AddToPlaylistMenu(
            listing: nil,
            onAdd: { _ in },
            onNewPlaylist: {}
        )
        #expect(unread.entries.isEmpty)
        let noProfile = AddToPlaylistMenu(
            listing: BridgePlaylistListing(
                listed: [],
                addTargets: [],
                canCreate: false
            ),
            onAdd: { _ in },
            onNewPlaylist: {}
        )
        #expect(noProfile.entries.isEmpty)
    }

    @Test("it lists the person's playlists, then New playlist…")
    func listsThePlaylistsThenNew() {
        var added: [String] = []
        var started = 0
        let menu = AddToPlaylistMenu(
            listing: BridgePlaylistListing(
                listed: [],
                addTargets: [summary("Late night"), summary("Running")],
                canCreate: true
            ),
            onAdd: { added.append($0) },
            onNewPlaylist: { started += 1 }
        )

        guard case .submenu(_, let items, _) = menu.entries[0] else {
            Issue.record("the first entry is the submenu")
            return
        }
        #expect(
            titles(items) == ["Late night", "Running", "-", "New playlist…"]
        )
        if case .item(let item) = items[1] { item.action() }
        if case .item(let item) = items[3] { item.action() }
        #expect(added == ["id-Running"])
        #expect(started == 1)
        #expect(titles([menu.entries[1]]) == ["-"])
    }

    @Test("with no playlist of the person's, it offers New playlist… alone")
    func offersNewAlone() {
        let menu = AddToPlaylistMenu(
            listing: BridgePlaylistListing(
                listed: [],
                addTargets: [],
                canCreate: true
            ),
            onAdd: { _ in },
            onNewPlaylist: {}
        )
        guard case .submenu(_, let items, _) = menu.entries[0] else {
            Issue.record("the first entry is the submenu")
            return
        }
        #expect(titles(items) == ["New playlist…"])
    }
}
