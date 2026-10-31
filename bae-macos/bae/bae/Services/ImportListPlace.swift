import BaeKit
import CoreGraphics
import Foundation

/// Where the person is in the import list: the row on top of the table and
/// how far the table is scrolled into it. The list slot holds it, so leaving
/// the import tab and coming back finds the list where it was left.
struct ImportListPlace: Equatable {
    /// The row on top, as far as the list had loaded it.
    enum Row: Equatable {
        /// A candidate's row, which core can say where it sits now.
        case candidate(stableKey: String, candidateKey: Data)
        /// A group header or a broken folder's row, found again while the
        /// list holds it.
        case entry(stableKey: String)
        /// A row whose page had not landed, known by its position alone.
        case unloaded
    }

    let row: Row
    /// Where the row was when the place was taken.
    let position: Int
    /// How far into the row the table was scrolled, in points.
    let offset: CGFloat
    /// What the list was narrowed by when the place was taken. A list
    /// narrowed otherwise since is a list narrowed anew, which opens at its
    /// first selected row, or its top.
    let narrowing: BridgeImportListNarrowing

    /// Where the row sits in `list` now, when `list` has loaded it.
    @MainActor
    func loadedPosition(
        in list: PaginatedList<BridgeImportListItem>
    ) -> Int? {
        switch row {
        case .candidate(let stableKey, _), .entry(let stableKey):
            list.position(of: stableKey)
        case .unloaded:
            nil
        }
    }
}

/// What a delivery of the list's content was read as: the revision it
/// brought the list to, what the view narrowed it by, and the row its view
/// opened at.
struct ImportCandidateListContentRead: Equatable {
    let revision: UInt64
    let narrowing: BridgeImportListNarrowing
    let openedAt: Int
}

/// Where the table scrolls once the list's content changes.
enum ImportListContentScroll: Equatable {
    /// Keep the person's place: the row that was on top stays on top.
    case keepPlace
    /// Put this row on top: the list was narrowed anew.
    case showOnTop(Int)
}

/// The content the table last took, which a new delivery is read against.
struct ImportCandidateTableContent {
    private var taken: ImportCandidateListContentRead

    init(_ read: ImportCandidateListContentRead) {
        taken = read
    }

    /// What the list was narrowed by in the content last taken.
    var narrowing: BridgeImportListNarrowing { taken.narrowing }

    /// Take `read`, and say where the table scrolls for it: nowhere while a
    /// reveal does the scrolling or the rows are the ones already shown.
    /// Narrowed anew — a different tab, filter text or filter entry — the
    /// list shows the row its view opened at, its first selected row or its
    /// top: the row on top before may be gone, or anywhere. Otherwise it
    /// keeps the person's place.
    mutating func take(
        _ read: ImportCandidateListContentRead,
        revealing: Bool
    ) -> ImportListContentScroll? {
        defer { taken = read }
        guard !revealing, read.revision != taken.revision else { return nil }
        if read.narrowing != taken.narrowing {
            return .showOnTop(read.openedAt)
        }
        return .keepPlace
    }
}
