import Foundation

/// One slot of the album grid, which places its slots left to right and top
/// to bottom, `columnCount` to a row.
enum AlbumGridCell: Identifiable, Equatable {
    /// The album at `position` in the list.
    case album(position: Int, albumId: String)
    /// A position with no album to show: its page has not landed, or it
    /// holds an album an earlier position already shows.
    case placeholder(position: Int)
    /// The open album's detail, first in the row under that album's row and
    /// drawn across the whole row.
    case detail(albumId: String)
    /// An empty slot completing a row the detail needs whole: the open
    /// album's row when it is the grid's short last row, and the detail's own.
    case filler(albumId: String, index: Int)

    /// A slot's identity. An album is itself wherever it sits, so a column
    /// count that moves it to another row moves the same view there. The grid
    /// opens one detail at a time, so the detail is one slot whichever album
    /// it shows: opening another album of the same row changes what it shows
    /// in place, and one of another row moves it there.
    enum Identity: Hashable {
        case album(String)
        case placeholder(Int)
        case detail
        /// The filler at this place among the detail's slots.
        case filler(Int)
    }

    var id: Identity {
        switch self {
        case .album(_, let albumId): .album(albumId)
        case .placeholder(let position): .placeholder(position)
        case .detail: .detail
        case .filler(_, let index): .filler(index)
        }
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
    /// The album each loaded position shows.
    private let albumIds: [Int: String]
    private let expansion: Expansion?

    /// `loaded` is every loaded position with its album, in position order.
    /// An album loaded at two positions — a page delivered before the page it
    /// left — shows at the first; the other position waits as a placeholder,
    /// since one album is one slot.
    init(
        totalCount: Int,
        columnCount: Int,
        loaded: [(position: Int, id: String)],
        openAlbumId: String?
    ) {
        precondition(columnCount > 0, "the grid has at least one column")
        self.totalCount = totalCount
        var albumIds: [Int: String] = [:]
        var shown: Set<String> = []
        var openPosition: Int?
        for (position, albumId) in loaded
        where position < totalCount && shown.insert(albumId).inserted {
            albumIds[position] = albumId
            if albumId == openAlbumId {
                openPosition = position
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
            return cell(forPosition: index)
        }
        let slot = index - expansion.insertAt
        guard slot < expansion.count else {
            return cell(forPosition: index - expansion.count)
        }
        return slot == expansion.leadingFillers
            ? .detail(albumId: expansion.albumId)
            : .filler(albumId: expansion.albumId, index: slot)
    }

    /// The slot showing list position `position`.
    func cell(forPosition position: Int) -> AlbumGridCell {
        albumIds[position].map { .album(position: position, albumId: $0) }
            ?? .placeholder(position: position)
    }
}
