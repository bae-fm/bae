import SwiftUI

/// The fills a made-up release is drawn in, from the theme's
/// `releaseFill…` roles.
enum ReleaseTone {
    case blue
    case orchid
    case pink
    case coral
    case gold
    case lime
    case mint
    case sky

    var color: Color {
        switch self {
        case .blue: Theme.releaseFillBlue
        case .orchid: Theme.releaseFillOrchid
        case .pink: Theme.releaseFillPink
        case .coral: Theme.releaseFillCoral
        case .gold: Theme.releaseFillGold
        case .lime: Theme.releaseFillLime
        case .mint: Theme.releaseFillMint
        case .sky: Theme.releaseFillSky
        }
    }
}
