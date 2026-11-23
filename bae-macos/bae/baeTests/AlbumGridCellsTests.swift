import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("AlbumGridCells")
struct AlbumGridCellsTests {
    @MainActor
    @Test("each grouped card reads its title, other artists and year together")
    func groupedCardCredits() async throws {
        let library = Library(
            handle: AlbumCreditsHandle(rows: groupedCreditRows)
        )
        let read = try library.albumList([], true).view(0, 1).values
        let delivery = try await read.next()
        try await read.cancel()
        let section = BridgeLibraryBrowseSection(
            id: "artist-a",
            title: "Artist A",
            image: nil,
            firstPosition: 0,
            albumCount: 2
        )
        let hosted = try await HostedAlbumGrid.host(
            preloaded: true,
            sections: [section],
            rows: delivery.rows
        ) { list in
            MacAlbumGrid(
                list: list,
                sortCriteria: [],
                groupByArtist: true,
                fullWidth: true,
                selection: AlbumGridSelection(),
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
                onDeleteAlbum: { _ in },
                expansionContent: { _ in EmptyView() }
            )
            .transaction { $0.disablesAnimations = true }
        }
        defer { hosted.window.close() }
        let tree = try AccessibilityTree.read(
            of: hosted.window,
            subject: "album credits in a grouped grid"
        )
        let texts = tree.nodes.compactMap {
            node -> (String, AccessibilityTree.Node)? in
            guard case .text(let text) = node.value else { return nil }
            return (text, node)
        }
        let duet = try #require(texts.first { $0.0 == "Duet Album" }?.1)
        let solo = try #require(texts.first { $0.0 == "Solo Album" }?.1)
        let credits = texts.filter { $0.0 == "Artist B" }
        #expect(credits.count == 1)
        let credit = try #require(credits.first?.1)
        #expect(credit.x >= duet.x && credit.x < solo.x)
        #expect(credit.y > duet.y)
        let tileTexts = [
            "Duet Album", "Artist B", "2024", "Solo Album", "2023",
        ]
        #expect(texts.map(\.0).filter(tileTexts.contains) == tileTexts)
        #expect(!texts.contains { $0.0 == "Artist A and Artist B" })
        #expect(!texts.contains { $0.0 == "Artist A" })
    }

    private var groupedCreditRows: [BridgeAlbumAppearance] {
        [
            BridgeAlbumAppearance(
                album: makeBridgeAlbum(
                    id: "duet",
                    title: "Duet Album",
                    year: 2024,
                    artistNames: "Artist A and Artist B"
                ),
                primaryReleaseUnplayable: false,
                displayArtistLine: .named("Artist B")
            ),
            BridgeAlbumAppearance(
                album: makeBridgeAlbum(
                    id: "solo",
                    title: "Solo Album",
                    year: 2023,
                    artistNames: "Artist A"
                ),
                primaryReleaseUnplayable: false,
                displayArtistLine: nil
            ),
        ]
    }

    @Test("artist headings and albums start full rows with one open appearance")
    func groupedLayoutRows() throws {
        let sections = [
            BridgeLibraryBrowseSection(
                id: "artist-a",
                title: "Artist A",
                image: nil,
                firstPosition: 0,
                albumCount: 3
            ),
            BridgeLibraryBrowseSection(
                id: "artist-b",
                title: "Artist B",
                image: nil,
                firstPosition: 3,
                albumCount: 2
            ),
        ]
        let ids = ["shared", "a", "b", "shared", "c"]
        for columns in 1...5 {
            let cells = AlbumGridLayout(
                totalCount: 5,
                sections: sections,
                columnCount: columns,
                idAt: { ids[$0] },
                open: AlbumGridOpenCard(albumId: "shared", position: 3)
            )
            let headings = cells.indices.filter {
                if case .heading = cells[$0] { return true }
                return false
            }
            #expect(headings.count == 2)
            #expect(headings.allSatisfy { $0 % columns == 0 })
            #expect(
                cells.filter {
                    if case .detail = $0 { return true }
                    return false
                }
                .count == 1
            )
            #expect(
                cells.filter {
                    if case .album = $0 { return true }
                    return false
                }
                .count == 5
            )
            #expect(Set(cells.map(\.id)).count == cells.count)
            let detail = try #require(
                cells.firstIndex(of: .detail(albumId: "shared"))
            )
            #expect(detail % columns == 0)
            // The detail opens in the second artist's group, under the card
            // opened there.
            let second = try #require(
                cells.firstIndex(
                    of: .heading(sectionId: "artist-b", title: "Artist B")
                )
            )
            #expect(detail > second)
        }
    }

    @Test("a collaboration has a distinct card identity in each artist grid")
    func collaborationIdentities() {
        let idAt: (Int) -> String? = { _ in "shared" }
        let open = AlbumGridOpenCard(albumId: "shared", position: 1)
        let first = AlbumGridCells(
            positions: 0..<1,
            columnCount: 2,
            idAt: idAt,
            open: open,
            sectionId: "artist-a"
        )
        let second = AlbumGridCells(
            positions: 1..<2,
            columnCount: 2,
            idAt: idAt,
            open: open,
            sectionId: "artist-b"
        )
        #expect(first[0].id == .album("shared", sectionId: "artist-a"))
        #expect(second[0].id == .album("shared", sectionId: "artist-b"))
        #expect(first[0].id != second[0].id)
        #expect(!first.contains(.detail(albumId: "shared")))
        #expect(second.contains(.detail(albumId: "shared")))
    }

    @Test("artist grids place expansion relative to their own first album")
    func expansionInArtistSection() {
        let cells = AlbumGridCells(
            positions: 5..<9,
            columnCount: 3,
            idAt: loaded(0..<12),
            open: open(8)
        )
        #expect(
            Array(cells.prefix(4)) == [
                .album(position: 5, albumId: "a5"),
                .album(position: 6, albumId: "a6"),
                .album(position: 7, albumId: "a7"),
                .album(position: 8, albumId: "a8"),
            ]
        )
        #expect(cells.firstIndex(of: .detail(albumId: "a8")) == 6)
        #expect(cells.count == 9)
        let other = AlbumGridCells(
            positions: 9..<12,
            columnCount: 3,
            idAt: loaded(0..<12),
            open: open(8)
        )
        #expect(other.count == 3)
    }
}

extension AlbumGridCellsTests {
    /// Albums "a0", "a1", … at their own positions, held at `positions`.
    private func loaded(_ positions: Range<Int>) -> (Int) -> String? {
        { positions.contains($0) ? "a\($0)" : nil }
    }

    /// The card of album "a`position`", opened at its position.
    private func open(_ position: Int) -> AlbumGridOpenCard {
        AlbumGridOpenCard(albumId: "a\(position)", position: position)
    }

    private func ids(_ cells: AlbumGridCells) -> [AlbumGridCell.Identity] {
        cells.map(\.id)
    }

    @Test("every position in order: loaded ones as albums, the rest waiting")
    func positionsInOrder() {
        let cells = AlbumGridCells(
            totalCount: 5,
            columnCount: 3,
            idAt: loaded(0..<3),
            open: nil
        )
        #expect(
            Array(cells) == [
                .album(position: 0, albumId: "a0"),
                .album(position: 1, albumId: "a1"),
                .album(position: 2, albumId: "a2"),
                .placeholder(position: 3),
                .placeholder(position: 4),
            ]
        )
    }

    @Test("the open album's detail takes the whole row under its row")
    func detailTakesTheNextRow() {
        let cells = AlbumGridCells(
            totalCount: 7,
            columnCount: 3,
            idAt: loaded(0..<7),
            open: open(4)
        )
        #expect(
            Array(cells) == [
                .album(position: 0, albumId: "a0"),
                .album(position: 1, albumId: "a1"),
                .album(position: 2, albumId: "a2"),
                .album(position: 3, albumId: "a3"),
                .album(position: 4, albumId: "a4"),
                .album(position: 5, albumId: "a5"),
                .detail(albumId: "a4"),
                .filler(albumId: "a4", index: 1),
                .filler(albumId: "a4", index: 2),
                .album(position: 6, albumId: "a6"),
            ]
        )
    }

    @Test("an open album in a short last row gets that row completed first")
    func detailAfterShortLastRow() {
        let cells = AlbumGridCells(
            totalCount: 5,
            columnCount: 3,
            idAt: loaded(0..<5),
            open: open(4)
        )
        #expect(
            Array(cells.dropFirst(5)) == [
                .filler(albumId: "a4", index: 0),
                .detail(albumId: "a4"),
                .filler(albumId: "a4", index: 2),
                .filler(albumId: "a4", index: 3),
            ]
        )
        // The detail starts a row.
        #expect(
            cells.firstIndex(of: .detail(albumId: "a4")).map { $0 % 3 } == 0
        )
    }

    @Test("a new column count moves albums without changing who they are")
    func columnCountKeepsIdentity() {
        let four = AlbumGridCells(
            totalCount: 10,
            columnCount: 4,
            idAt: loaded(0..<10),
            open: open(5)
        )
        let three = AlbumGridCells(
            totalCount: 10,
            columnCount: 3,
            idAt: loaded(0..<10),
            open: open(5)
        )
        let albums: (AlbumGridCells) -> [AlbumGridCell.Identity] = { cells in
            cells.map(\.id)
                .filter {
                    if case .album = $0 { return true }
                    return false
                }
        }
        #expect(albums(four) == albums(three))
        #expect(four.contains(.detail(albumId: "a5")))
        #expect(three.contains(.detail(albumId: "a5")))
        // Four columns put the detail after position 7, three after 5.
        #expect(four.firstIndex(of: .detail(albumId: "a5")) == 8)
        #expect(three.firstIndex(of: .detail(albumId: "a5")) == 6)
    }

    @Test(
        "an open card the window does not hold opens its detail under its row"
    )
    func unloadedOpenCardOpensItsDetail() {
        let cells = AlbumGridCells(
            totalCount: 6,
            columnCount: 3,
            idAt: loaded(0..<3),
            open: AlbumGridOpenCard(albumId: "elsewhere", position: 4)
        )
        #expect(cells.count == 9)
        #expect(cells.firstIndex(of: .detail(albumId: "elsewhere")) == 6)
        #expect(cells[4] == .placeholder(position: 4))
    }

    @Test("positions past the total are not shown")
    func loadedPastTotalIgnored() {
        let cells = AlbumGridCells(
            totalCount: 2,
            columnCount: 2,
            idAt: loaded(0..<4),
            open: open(3)
        )
        #expect(
            Array(cells) == [
                .album(position: 0, albumId: "a0"),
                .album(position: 1, albumId: "a1"),
            ]
        )
    }

    @Test("slot identities are unique with a detail open at any column count")
    func identitiesUnique() {
        for columns in 1...6 {
            for open in 0..<9 {
                let cells = AlbumGridCells(
                    totalCount: 9,
                    columnCount: columns,
                    idAt: loaded(0..<9),
                    open: self.open(open)
                )
                #expect(Set(ids(cells)).count == cells.count)
                let detail = cells.firstIndex(of: .detail(albumId: "a\(open)"))
                #expect(detail.map { $0 % columns } == 0)
            }
        }
    }

    @Test(
        "opening another album of the same row keeps the detail's slot where it is"
    )
    func sameRowSwapKeepsTheDetailSlot() {
        let first = AlbumGridCells(
            totalCount: 9,
            columnCount: 3,
            idAt: loaded(0..<9),
            open: open(3)
        )
        let second = AlbumGridCells(
            totalCount: 9,
            columnCount: 3,
            idAt: loaded(0..<9),
            open: open(5)
        )
        #expect(ids(first) == ids(second))
        #expect(
            first.firstIndex(of: .detail(albumId: "a3"))
                == second.firstIndex(of: .detail(albumId: "a5"))
        )
    }
}

/// Answers the real library list adapter with the album rows core sends.
private final class AlbumCreditsHandle: AppHandle, @unchecked Sendable {
    private let rows: [BridgeAlbumAppearance]

    init(rows: [BridgeAlbumAppearance]) {
        self.rows = rows
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("The album credits fixture needs rows")
    }

    override func subscribeListView(
        list: BridgeList,
        first: UInt64,
        last: UInt64
    ) throws -> ListViewSubscription {
        AlbumCreditsSubscription(rows: rows)
    }
}

private final class AlbumCreditsSubscription: ListViewSubscription,
    @unchecked Sendable
{
    private let deliveries: FixedSubscription<BridgeListView>

    init(rows: [BridgeAlbumAppearance]) {
        deliveries = FixedSubscription(
            value: BridgeListView(start: 0, rows: .albums(rows: rows))
        )
        super.init(noHandle: ListViewSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("The album credits fixture needs rows")
    }

    override func next() async throws -> BridgeListView {
        try await deliveries.next()
    }

    override func cancel() async throws { try await deliveries.cancel() }
}
