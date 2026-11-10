import SwiftUI

/// Where one part of the sheep's drawing stands: moved by `x` and `y` in the
/// drawing's units, turned by `turn` degrees, scaled by `scaleX` and `scaleY`
/// around the part's pivot, and drawn at `opacity`. Rest is no change.
struct SheepPose: Equatable {
    var x = 0.0
    var y = 0.0
    var turn = 0.0
    var scaleX = 1.0
    var scaleY = 1.0
    var opacity = 1.0

    static let rest = SheepPose()

    /// A pose written the way the design's keys write it: moved by `x`, `y`,
    /// turned by `turn` degrees, scaled by `scale` and, when `squash` is
    /// given, by `squash` vertically.
    static func at(
        _ x: Double = 0,
        _ y: Double = 0,
        _ turn: Double = 0,
        _ scale: Double = 1,
        _ squash: Double? = nil
    ) -> SheepPose {
        SheepPose(
            x: x,
            y: y,
            turn: turn,
            scaleX: scale,
            scaleY: squash ?? scale
        )
    }

    /// The same pose drawn at `opacity`.
    func opacity(_ opacity: Double) -> SheepPose {
        var pose = self
        pose.opacity = opacity
        return pose
    }

    /// The pose `fraction` of the way to `other`, each part of it on its own.
    func blended(to other: SheepPose, _ fraction: Double) -> SheepPose {
        func mix(_ from: Double, _ to: Double) -> Double {
            from + (to - from) * fraction
        }
        return SheepPose(
            x: mix(x, other.x),
            y: mix(y, other.y),
            turn: mix(turn, other.turn),
            scaleX: mix(scaleX, other.scaleX),
            scaleY: mix(scaleY, other.scaleY),
            opacity: mix(opacity, other.opacity)
        )
    }
}

/// How a move from one key to the next speeds up and slows down: the
/// design's curves, by the letters its keys give them.
enum SheepEase {
    /// Slow at both ends.
    case inOut
    /// Fast at first, then settling.
    case out
    /// Slow at first, then fast.
    case `in`
    case linear
    /// Overshoots its end and comes back.
    case back
    /// Barely moves, then snaps to its end.
    case snap
    /// Stays where it was until the next key.
    case hold

    /// How far along the move is, from 0 to 1, when `time` of it has passed.
    func progress(_ time: Double) -> Double {
        switch self {
        case .linear: time
        case .inOut: Self.curve(0.45, 0, 0.55, 1, time)
        case .out: Self.curve(0.2, 0.7, 0.3, 1, time)
        case .in: Self.curve(0.55, 0, 0.85, 0.35, time)
        case .back: Self.curve(0.3, 1.5, 0.55, 1, time)
        case .snap: Self.curve(0.8, 0, 1, 0.6, time)
        case .hold: 0
        }
    }

    /// A CSS `cubic-bezier(x1, y1, x2, y2)`.
    private static func curve(
        _ x1: Double,
        _ y1: Double,
        _ x2: Double,
        _ y2: Double,
        _ time: Double
    ) -> Double {
        UnitCurve.bezier(
            startControlPoint: UnitPoint(x: x1, y: y1),
            endControlPoint: UnitPoint(x: x2, y: y2)
        )
        .value(at: time)
    }
}

/// A pose a part takes at `time` seconds into a moment, and how it moves on
/// from there to the next key.
struct SheepKey {
    let time: Double
    let pose: SheepPose
    let ease: SheepEase

    init(_ time: Double, _ pose: SheepPose, _ ease: SheepEase = .inOut) {
        self.time = time
        self.pose = pose
        self.ease = ease
    }
}

/// One part's keys through a moment of `duration` seconds.
struct SheepTrack {
    private let keys: [SheepKey]

    /// A part that doesn't hold still to the end of the moment goes back to
    /// its first pose by then.
    init(_ keys: [SheepKey], duration: Double) {
        var ordered = keys.enumerated()
            .sorted {
                ($0.element.time, $0.offset) < ($1.element.time, $1.offset)
            }
            .map(\.element)
        if let first = ordered.first, let last = ordered.last,
            last.time < duration
        {
            ordered.append(SheepKey(duration, first.pose, .linear))
        }
        self.keys = ordered
    }

    func pose(at time: Double) -> SheepPose {
        guard let first = keys.first, time > first.time else {
            return keys.first?.pose ?? .rest
        }
        guard let next = keys.firstIndex(where: { $0.time > time }) else {
            return keys[keys.count - 1].pose
        }
        let from = keys[next - 1]
        let to = keys[next]
        let along = (time - from.time) / (to.time - from.time)
        return from.pose.blended(to: to.pose, from.ease.progress(along))
    }
}
