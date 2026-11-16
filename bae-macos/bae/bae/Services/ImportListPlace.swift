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
        /// A group header's row, a broken folder's, or a row whose page had
        /// not landed: core can't say where it sits now, so the place is
        /// its position.
        case atPosition
    }

    let row: Row
    /// Where the row was when the place was taken.
    let position: Int
    /// How far into the row the table was scrolled, in points.
    let offset: CGFloat
}
