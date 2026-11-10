import SwiftUI

/// The sheep's new drawings, from the design's path data: the mouth opened
/// wide and the munching mouth, the blush lines, the marks of surprise, the
/// stars and the floating heart, the streaks of air, and the strokes of a
/// release's drawing. Each is in the units of the part it belongs to.
enum SheepShapes {
    static let gape = Path(
        svg:
            "M184 266C184 258 268 258 268 266C270 312 246 338 226 338C206 338 182 312 184 266Z"
    )
    static let tongue = Path(
        svg: "M198 322C206 304 246 304 254 322C246 334 206 334 198 322Z"
    )
    static let munch = Path(svg: "M196 270q15 20 30 4q15 16 30-4")
    static let blush = Path(
        svg:
            "M108 262l-10 14M124 262l-10 14M140 262l-10 14M322 273l-10 14M338 273l-10 14M354 273l-10 14"
    )
    static let surpriseRight = Path(
        svg: "M452 -6l22-40M492 22l40-22M500 72l44 2"
    )
    static let surpriseLeft = Path(
        svg: "M28 -6l-22-40M-12 22l-40-22M-20 72l-44 2"
    )
    static let surpriseTop = Path(
        svg: "M180 -20l-14-40M240 -30v-44M300 -20l14-40"
    )
    static let star = Path(
        svg: "M0-34C4-8 8-4 34 0C8 4 4 8 0 34C-4 8-8 4-34 0C-8-4-4-8 0-34Z"
    )
    static let floatingHeart = Path(
        svg:
            "M0 14C-14 2-26-8-26-20C-26-34-8-38 0-24C8-38 26-34 26-20C26-8 14 2 0 14Z"
    )
    static let wind = Path(svg: "M-70 0Q0-18 70 0")

    // A release, drawn to fit a circle of radius 50 around the origin.

    /// A record: a disc with a hole in its middle.
    static let disc = Path(
        svg:
            "M0-50a50 50 0 1 1 0 100a50 50 0 1 1 0-100ZM0-5a5 5 0 1 0 0 10a5 5 0 1 0 0-10Z"
    )
    static let shine = Path(svg: "M-30-26a40 40 0 0 1 22-12")
    /// A record's label, with the hole through it.
    static let label = Path(
        svg:
            "M0-19a19 19 0 1 1 0 38a19 19 0 1 1 0-38ZM0-5a5 5 0 1 0 0 10a5 5 0 1 0 0-10Z"
    )
    static let cdDisc = Path(
        svg:
            "M0-50a50 50 0 1 1 0 100a50 50 0 1 1 0-100ZM0-10a10 10 0 1 0 0 20a10 10 0 1 0 0-20Z"
    )
    static let cdSkyArc = Path(svg: "M-34-22a40 40 0 0 1 30-18")
    static let cdToneArc = Path(svg: "M34 22a40 40 0 0 1-30 18")
    static let coverWave = Path(svg: "M-45 26C-22 10 6 34 45 16")
}
