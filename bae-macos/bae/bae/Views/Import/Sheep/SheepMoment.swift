import SwiftUI

/// What moves in the sheep's drawing. Each is a group of the drawing with its
/// own pivot (`ImportSheepPainter` lists them); a moment gives the ones it
/// moves a track.
enum SheepPart: Hashable {
    /// The whole sheep turning over on the spot.
    case roll
    /// The sheep without the releases, squashing and leaping together.
    case rig
    case head
    case earLeft
    case earRight
    /// The eyes, cheeks and mouth together, sliding inside the face.
    case features
    case cheekLeft
    case cheekRight
    case mouth
    /// The mouth opened wide.
    case gape
    /// The closed mouth, chewing.
    case munch
    case eyes
    case heart
    /// The arms and the player they hold.
    case arms
    /// Marks of surprise beside the head.
    case surprise(Side)
    case blush
    /// The player's screen lighting up.
    case flash
    /// The heart that floats up.
    case floatingHeart
    case star(Int)
    /// A streak of air rushing toward the mouth.
    case wind(Int)
    /// Where the moment's release `n` is.
    case release(Int)
    /// How the moment's release `n` sways or turns where it is.
    case spin(Int)

    enum Side {
        case left
        case right
        case top
    }

    /// The pose of a part its moment gives no track: parts that draw
    /// something only while they move start unseen.
    var rest: SheepPose {
        switch self {
        case .munch, .gape, .blush, .floatingHeart, .flash, .surprise,
            .release, .wind:
            .rest.opacity(0)
        case .star:
            .at(0, 0, 0, 0)
        default:
            .rest
        }
    }
}

/// A made-up release the sheep takes in: its drawing, and how big it is.
struct SheepRelease {
    enum Kind {
        case cover
        case vinyl
        case cd
        /// A record half out of its sleeve; the record's own tone.
        case sleeve(disc: ReleaseTone)
    }

    let kind: Kind
    let tone: ReleaseTone
    /// The radius of the circle it fits in, in the drawing's units.
    let radius: Double
}

/// One moment of the sheep's loop: `duration` seconds, its rest included, in
/// which some parts of the drawing move and some releases pass through.
struct SheepMoment {
    let duration: Double
    let releases: [SheepRelease]
    private let tracks: [SheepPart: SheepTrack]

    init(
        duration: Double,
        releases: [SheepRelease] = [],
        tracks: [SheepPart: [SheepKey]]
    ) {
        self.duration = duration
        self.releases = releases
        self.tracks = tracks.mapValues { SheepTrack($0, duration: duration) }
    }

    func pose(of part: SheepPart, at time: Double) -> SheepPose {
        tracks[part]?.pose(at: time) ?? part.rest
    }
}
