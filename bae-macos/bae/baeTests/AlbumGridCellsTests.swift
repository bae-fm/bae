import BaeKit
import Testing

@testable import bae

@Suite("AlbumGridCells")
struct AlbumGridCellsTests {
    @Test("artist headings and albums start full rows with one open appearance")
    func groupedLayoutRows() throws {
        let sections = [
            BridgeLibraryBrowseSection(
                id: "artist-a",
                title: "Artist A",
                window: .init(offset: 0, limit: 3)
            ),
            BridgeLibraryBrowseSection(
                id: "artist-b",
                title: "Artist B",
                window: .init(offset: 3, limit: 2)
            ),
        ]
        let loaded: [(position: Int, id: String)] = [
            (0, "shared"), (1, "a"), (2, "b"), (3, "shared"), (4, "c"),
        ]
        for columns in 1...5 {
            let cells = AlbumGridLayout(
                totalCount: 5,
                sections: sections,
                columnCount: columns,
                loaded: loaded,
                openAlbumId: "shared",
                openSectionId: "artist-b"
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
        }
    }

    @Test("a collaboration has a distinct card identity in each artist grid")
    func collaborationIdentities() {
        let loaded: [(position: Int, id: String)] = [
            (0, "shared"), (1, "shared"),
        ]
        let first = AlbumGridCells(
            positions: 0..<1,
            columnCount: 2,
            loaded: loaded,
            openAlbumId: nil,
            sectionId: "artist-a"
        )
        let second = AlbumGridCells(
            positions: 1..<2,
            columnCount: 2,
            loaded: loaded,
            openAlbumId: "shared",
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
            loaded: loaded(0..<12),
            openAlbumId: "a8"
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
            loaded: loaded(0..<12),
            openAlbumId: "a8"
        )
        #expect(other.count == 3)
    }

}

extension AlbumGridCellsTests {
    private typealias Loaded = [(position: Int, id: String)]

    /// Albums "a0", "a1", … loaded at their own positions.
    private func loaded(_ positions: Range<Int>) -> Loaded {
        positions.map { (position: $0, id: "a\($0)") }
    }

    private func ids(_ cells: AlbumGridCells) -> [AlbumGridCell.Identity] {
        cells.map(\.id)
    }

    @Test("every position in order: loaded ones as albums, the rest waiting")
    func positionsInOrder() {
        let cells = AlbumGridCells(
            totalCount: 5,
            columnCount: 3,
            loaded: loaded(0..<3),
            openAlbumId: nil
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
            loaded: loaded(0..<7),
            openAlbumId: "a4"
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
            loaded: loaded(0..<5),
            openAlbumId: "a4"
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
            loaded: loaded(0..<10),
            openAlbumId: "a5"
        )
        let three = AlbumGridCells(
            totalCount: 10,
            columnCount: 3,
            loaded: loaded(0..<10),
            openAlbumId: "a5"
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

    @Test("an album loaded at two positions shows once, at the first")
    func duplicateAlbumShowsOnce() {
        let cells = AlbumGridCells(
            totalCount: 4,
            columnCount: 2,
            loaded: [
                (position: 0, id: "x"), (position: 1, id: "y"),
                (position: 2, id: "x"), (position: 3, id: "z"),
            ],
            openAlbumId: nil
        )
        #expect(
            Array(cells) == [
                .album(position: 0, albumId: "x"),
                .album(position: 1, albumId: "y"),
                .placeholder(position: 2),
                .album(position: 3, albumId: "z"),
            ]
        )
        #expect(Set(ids(cells)).count == cells.count)
    }

    @Test("an open album that is not loaded opens no detail")
    func unloadedOpenAlbumHasNoDetail() {
        let cells = AlbumGridCells(
            totalCount: 6,
            columnCount: 3,
            loaded: loaded(0..<3),
            openAlbumId: "elsewhere"
        )
        #expect(cells.count == 6)
        #expect(!cells.contains(.detail(albumId: "elsewhere")))
    }

    @Test("positions past the total are not shown")
    func loadedPastTotalIgnored() {
        let cells = AlbumGridCells(
            totalCount: 2,
            columnCount: 2,
            loaded: loaded(0..<4),
            openAlbumId: "a3"
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
                    loaded: loaded(0..<9),
                    openAlbumId: "a\(open)"
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
            loaded: loaded(0..<9),
            openAlbumId: "a3"
        )
        let second = AlbumGridCells(
            totalCount: 9,
            columnCount: 3,
            loaded: loaded(0..<9),
            openAlbumId: "a5"
        )
        #expect(ids(first) == ids(second))
        #expect(
            first.firstIndex(of: .detail(albumId: "a3"))
                == second.firstIndex(of: .detail(albumId: "a5"))
        )
    }
}
