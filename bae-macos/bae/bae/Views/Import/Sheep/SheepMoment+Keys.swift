import SwiftUI

/// Shared pieces the moments' keys are written from.
extension SheepMoment {
    typealias Keys = [SheepKey]

    /// The ears turn opposite ways from each other: the left ear rises with a
    /// positive angle, the right with a negative one.
    static let ears: [(part: SheepPart, sign: Double)] = [
        (.earLeft, 1), (.earRight, -1),
    ]

    /// The tracks of several groups of parts as one; no part is in two groups.
    static func merged(_ groups: [[SheepPart: Keys]]) -> [SheepPart: Keys] {
        Dictionary(groups.flatMap { $0 }) { _, _ in
            preconditionFailure("two tracks for one part")
        }
    }

    /// Keys along a path: `path(u)` gives the pose for `u` from 0 to 1, spread
    /// over `start`…`end` at about every `step` seconds, as straight moves.
    static func along(
        _ start: Double,
        _ end: Double,
        every step: Double,
        _ path: (Double) -> SheepPose
    ) -> Keys {
        let count = max(2, Int(((end - start) / step).rounded()))
        return (0...count)
            .map { index in
                let along = Double(index) / Double(count)
                return SheepKey(
                    start + (end - start) * along,
                    path(along),
                    .linear
                )
            }
    }

    /// The face turned toward a side: the features slide that way, and the
    /// far cheek narrows while the near one widens.
    static func faceToward(
        _ side: Double,
        on: (Double, Double),
        off: (Double, Double),
        shift: Double
    ) -> Keys {
        [
            SheepKey(0, .rest), SheepKey(on.0, .rest),
            SheepKey(on.1, .at(shift * side, 0)),
            SheepKey(off.0, .at(shift * side, 0)), SheepKey(off.1, .rest),
        ]
    }

    /// The closed, munching mouth: it shows from `from`, chews at each of
    /// `chews`, and gives way to the usual mouth at `end`.
    static func munching(from: Double, chews: [Double], until end: Double)
        -> Keys
    {
        let chew = SheepPose.at(0, 4, 0, 1.15, 0.45)
        var keys: Keys = [
            SheepKey(0, .rest.opacity(0)),
            SheepKey(from - 0.03, .rest.opacity(0), .linear),
            SheepKey(from, .rest),
        ]
        for time in chews {
            keys.append(SheepKey(time, chew))
            keys.append(SheepKey(time + 0.25, .rest))
        }
        keys.append(SheepKey(end, .rest, .linear))
        keys.append(SheepKey(end + 0.08, .rest.opacity(0)))
        return keys
    }

    /// A swallow starting at `start`: the body squashes, stretches and
    /// settles.
    static func swallow(at start: Double) -> Keys {
        [
            SheepKey(start - 0.6, .rest),
            SheepKey(start, .rest, .in),
            SheepKey(start + 0.2, .at(0, 0, 0, 1.07, 0.92), .out),
            SheepKey(start + 0.45, .at(0, 0, 0, 0.97, 1.04)),
            SheepKey(start + 0.7, .at(0, 0, 0, 1.01, 0.99)),
            SheepKey(start + 0.95, .rest),
        ]
    }
}
