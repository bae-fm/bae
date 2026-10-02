import BaeKit
import Foundation

/// One slot of the album grid, which places its slots left to right and top
/// to bottom, `columnCount` to a row.
enum AlbumGridCell: Identifiable, Equatable {
    case heading(sectionId: String, title: String)
    case sectionFiller(sectionId: String, index: Int)
    /// The album at `position` in the list.
    case album(position: Int, albumId: String, sectionId: String? = nil)
    /// A position whose page has not landed.
    case placeholder(position: Int)
    /// The open album's detail, first in the row under that album's row and
    /// drawn across the whole row.
    case detail(albumId: String)
    /// An empty slot completing a row the detail needs whole: the open
    /// album's row when it is the grid's short last row, and the detail's own.
    case filler(albumId: String, index: Int)

    /// A list position is one slot whether its page has landed or not: a
    /// page landing or being let go changes what the slot shows, never which
    /// slot it is, so the lazy grid lays out the same slots either way. A
    /// column count that moves a position to another row moves the same
    /// view. The grid opens one detail at a time, so the detail is one slot
    /// whichever album it shows: opening another album of the same row
    /// changes what it shows in place, and one of another row moves it there.
    enum Identity: Hashable {
        case heading(String)
        case sectionFiller(String, Int)
        case position(Int)
        case detail
        /// The filler at this place among the detail's slots.
        case filler(Int)
    }

    var id: Identity {
        switch self {
        case .heading(let sectionId, _): .heading(sectionId)
        case .sectionFiller(let sectionId, let index):
            .sectionFiller(sectionId, index)
        case .album(let position, _, _), .placeholder(let position):
            .position(position)
        case .detail: .detail
        case .filler(_, let index): .filler(index)
        }
    }
}

/// One sequence of slots for the whole lazy grid. Headings occupy full rows,
/// and each artist starts a new album row. Only section boundaries and loaded
/// albums are held; unloaded album slots are produced as they are requested.
struct AlbumGridLayout: RandomAccessCollection {
    private struct Group {
        let heading: BridgeLibraryBrowseSection?
        let cells: AlbumGridCells
        let slots: Range<Int>
    }

    private let groups: [Group]
    private let columnCount: Int

    init(
        totalCount: Int,
        sections: [BridgeLibraryBrowseSection],
        columnCount: Int,
        loaded: [(position: Int, id: String)],
        openAlbumId: String?,
        openSectionId: String?
    ) {
        self.columnCount = columnCount
        if sections.isEmpty {
            let cells = AlbumGridCells(
                totalCount: totalCount,
                columnCount: columnCount,
                loaded: loaded,
                openAlbumId: openAlbumId
            )
            groups = [Group(heading: nil, cells: cells, slots: 0..<cells.count)]
        }
        else {
            var groups: [Group] = []
            var offset = 0
            for section in sections {
                let cells = AlbumGridCells(
                    positions: Int(
                        section.window.offset
                    )..<Int(section.window.offset + section.window.limit),
                    columnCount: columnCount,
                    loaded: loaded,
                    openAlbumId: openSectionId == section.id
                        ? openAlbumId : nil,
                    sectionId: section.id
                )
                let count =
                    columnCount
                    + ((cells.count + columnCount - 1) / columnCount)
                    * columnCount
                groups.append(
                    Group(
                        heading: section,
                        cells: cells,
                        slots: offset..<(offset + count)
                    )
                )
                offset += count
            }
            self.groups = groups
        }
    }

    var startIndex: Int { 0 }
    var endIndex: Int { groups.last?.slots.upperBound ?? 0 }

    /// The slot showing list position `position`, or nil if no section
    /// holds it.
    func slot(ofPosition position: Int) -> Int? {
        for group in groups {
            if let local = group.cells.slot(ofPosition: position) {
                let heading = group.heading == nil ? 0 : columnCount
                return group.slots.lowerBound + heading + local
            }
        }
        return nil
    }

    subscript(index: Int) -> AlbumGridCell {
        precondition(index >= startIndex && index < endIndex)
        var lower = 0
        var upper = groups.count
        while lower + 1 < upper {
            let middle = (lower + upper) / 2
            if groups[middle].slots.lowerBound <= index {
                lower = middle
            }
            else {
                upper = middle
            }
        }
        let group = groups[lower]
        let local = index - group.slots.lowerBound
        guard let heading = group.heading else { return group.cells[local] }
        if local == 0 {
            return .heading(sectionId: heading.id, title: heading.title)
        }
        if local < columnCount || local - columnCount >= group.cells.count {
            return .sectionFiller(sectionId: heading.id, index: local)
        }
        return group.cells[local - columnCount]
    }
}

/// The album grid's slots at one column count: every list position in order,
/// with the open album's detail taking the whole row under the row that
/// holds the album.
struct AlbumGridCells: RandomAccessCollection {
    private struct Expansion {
        let albumId: String
        /// The list position the detail's slots go before: the end of the
        /// open album's row.
        let insertAt: Int
        /// Fillers completing the open album's row before the detail.
        let leadingFillers: Int
        /// Every slot the detail takes: the leading fillers, the detail, and
        /// the fillers completing the detail's row.
        let count: Int
    }

    private let totalCount: Int
    private let startPosition: Int
    private let sectionId: String?
    /// The album each loaded position shows.
    private let albumIds: [Int: String]
    private let expansion: Expansion?

    /// `loaded` is every loaded position with its album, in position order.
    /// An album loaded at two positions — a page delivered before the page it
    /// left — opens its detail under the first.
    init(
        totalCount: Int,
        columnCount: Int,
        loaded: [(position: Int, id: String)],
        openAlbumId: String?
    ) {
        self.init(
            positions: 0..<totalCount,
            columnCount: columnCount,
            loaded: loaded,
            openAlbumId: openAlbumId
        )
    }

    init(
        positions: Range<Int>,
        columnCount: Int,
        loaded: [(position: Int, id: String)],
        openAlbumId: String?,
        sectionId: String? = nil
    ) {
        precondition(columnCount > 0, "the grid has at least one column")
        self.totalCount = positions.count
        self.startPosition = positions.lowerBound
        self.sectionId = sectionId
        var albumIds: [Int: String] = [:]
        var openPosition: Int?
        for (position, albumId) in loaded where positions.contains(position) {
            albumIds[position] = albumId
            if albumId == openAlbumId, openPosition == nil {
                openPosition = position - positions.lowerBound
            }
        }
        self.albumIds = albumIds
        guard let openAlbumId, let openPosition else {
            expansion = nil
            return
        }
        let insertAt = Swift.min(
            (openPosition / columnCount + 1) * columnCount,
            totalCount
        )
        let leadingFillers =
            (columnCount - insertAt % columnCount) % columnCount
        expansion = Expansion(
            albumId: openAlbumId,
            insertAt: insertAt,
            leadingFillers: leadingFillers,
            count: leadingFillers + columnCount
        )
    }

    var startIndex: Int { 0 }

    var endIndex: Int { totalCount + (expansion?.count ?? 0) }

    subscript(index: Int) -> AlbumGridCell {
        guard let expansion, index >= expansion.insertAt else {
            return cell(forPosition: startPosition + index)
        }
        let slot = index - expansion.insertAt
        guard slot < expansion.count else {
            return cell(forPosition: startPosition + index - expansion.count)
        }
        return slot == expansion.leadingFillers
            ? .detail(albumId: expansion.albumId)
            : .filler(albumId: expansion.albumId, index: slot)
    }

    /// The slot showing list position `position`, or nil if it is not one
    /// of these positions.
    func slot(ofPosition position: Int) -> Int? {
        let local = position - startPosition
        guard local >= 0, local < totalCount else { return nil }
        guard let expansion, local >= expansion.insertAt else { return local }
        return local + expansion.count
    }

    /// The slot showing list position `position`.
    func cell(forPosition position: Int) -> AlbumGridCell {
        albumIds[position]
            .map {
                .album(position: position, albumId: $0, sectionId: sectionId)
            }
            ?? .placeholder(position: position)
    }
}
