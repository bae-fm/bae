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
