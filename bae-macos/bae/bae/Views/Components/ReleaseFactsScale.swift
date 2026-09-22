import SwiftUI

/// The two sizes the release's records row is drawn at: full width in the
/// import pane, and packed into the library expansion's facts line, which
/// sits in a 320-point card rather than across a pane.
enum ReleaseFactsScale {
    case pane
    case card

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
