import SwiftUI

/// The logo's drawing cut into the parts the sheep moves, one vector image
/// each in the asset catalog's `SheepParts` folder. They come from
/// `design/system/assets/Logos/sheep-parts`, and all share the logo's 584 by
/// 545 box, so drawn at its origin they stack in place.
enum SheepArt: String, CaseIterable {
    case headband
    case earcupLeft = "earcup-left"
    case body
    case earLeft = "ear-left"
    case earRight = "ear-right"
    case face
    case cheekLeft = "cheek-left"
    case cheekRight = "cheek-right"
    case mouth
    case eyes
    case wool
    case earcupRight = "earcup-right"
    case player
    case heart
    case bubbles
    case handLeft = "hand-left"
    case handRight = "hand-right"
    /// The player's screen alone, solid white: it flashes over the player.
    case screenGlow = "screen-glow"
    /// The face's shape filled solid: the mask that keeps the features inside
    /// the face.
    case faceClip = "face-clip"

    /// The size of every part's box, the logo's.
    static let box = CGSize(width: 584, height: 545)

    var assetName: String { "SheepParts/\(rawValue)" }
}
