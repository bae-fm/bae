import SwiftUI

/// The two sizes the release's facts are drawn at: full width in the import
/// pane, and packed into the card behind a candidate row's glyphs or the
/// library expansion's facts line.
///
/// The mark and rip-match lines read the same at both; what the card packs is
/// the space between lines and the records row, which sits in a 320-point
/// card rather than across a pane.
enum ReleaseFactsScale {
    case pane
    case card

    /// The gap between one mark line and the next, and between the last mark
    /// and the rip-match line.
    var lineSpacing: CGFloat {
        switch self {
        case .pane: 7
        case .card: 6
        }
    }

    /// The gap between two record links on one row.
    var recordSpacing: CGFloat {
        switch self {
        case .pane: 14
        case .card: 12
        }
    }

    /// The gap between two rows of record links.
    var recordRowSpacing: CGFloat {
        switch self {
        case .pane: 6
        case .card: 4
        }
    }

    var recordFontSize: CGFloat {
        switch self {
        case .pane: 11.5
        case .card: 11
        }
    }
}
