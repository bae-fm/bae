import SwiftUI

extension SheepMoment {
    /// The heart on the player starts to pulse and the sheep bops to the
    /// music in its headphones for eight beats: nods, sways, three little
    /// hops, ears flapping. No release.
    static func bop() -> SheepMoment {
        let plan = Bop()
        return SheepMoment(
            duration: Bop.duration,
            tracks: merged([plan.beatTracks(), plan.faceTracks()])
        )
    }

    private struct Bop {
        static let duration = 7.0
        static let beat = 0.42
        static let first = 0.9

        private var beats: [Double] {
            (0..<8).map { Self.first + Double($0) * Self.beat }
        }
        private var end: Double { beats[7] + Self.beat }

        /// What moves with each beat: the head, the body's hops, the heart on
        /// the player and the ears.
        func beatTracks() -> [SheepPart: Keys] {
            let beat = Self.beat
            let first = Self.first
            var head: Keys = [SheepKey(0, .rest), SheepKey(first - 0.3, .rest)]
            var rig: Keys = [SheepKey(0, .rest), SheepKey(first - 0.2, .rest)]
            var heart: Keys = [
                SheepKey(0, .rest), SheepKey(first - 0.42, .rest),
            ]
            var earKeys: [SheepPart: Keys] = [:]
            for (ear, _) in ears {
                earKeys[ear] = [
                    SheepKey(0, .rest), SheepKey(first - 0.2, .rest),
                ]
            }
            for (index, start) in beats.enumerated() {
                let side: Double = index % 2 == 1 ? -1 : 1
                let hop = index == 2 || index == 3 || index == 6
                head.append(SheepKey(start, .at(3 * side, 8, 6 * side), .out))
                head.append(
                    SheepKey(start + beat * 0.55, .at(2 * side, -3, 4 * side))
                )
                // A hop leaves the ground on its beat and lands on the next
                // one, squashing there.
                if hop {
                    rig.append(SheepKey(start, .at(0, 0, 0, 1.06, 0.92), .out))
                    rig.append(
                        SheepKey(start + 0.16, .at(0, -34, 0, 0.95, 1.06), .in)
                    )
                    rig.append(SheepKey(start + beat - 0.03, .rest))
                }
                else {
                    rig.append(SheepKey(start, .at(0, 0, 0, 1.03, 0.97), .out))
                    rig.append(SheepKey(start + beat * 0.6, .rest))
                }
                heart.append(SheepKey(start - 0.02, .rest, .back))
                heart.append(SheepKey(start + 0.1, .at(0, 0, 0, 1.3)))
                heart.append(SheepKey(start + beat - 0.04, .rest))
                for (ear, sign) in ears {
                    earKeys[ear, default: []] += [
                        SheepKey(
                            start + 0.06,
                            .at(0, 0, (hop ? 18 : 9) * sign),
                            .out
                        ),
                        SheepKey(start + beat * 0.7, .at(0, 0, -4 * sign)),
                    ]
                }
            }
            var t: [SheepPart: Keys] = [
                .head: head + [
                    SheepKey(end, .at(0, 4, 0)), SheepKey(end + 0.35, .rest),
                ],
                .rig: rig + [SheepKey(end + 0.1, .rest)],
                .heart: heart + [SheepKey(end + 0.3, .rest)],
            ]
            for (ear, sign) in ears {
                t[ear] =
                    earKeys[ear, default: []] + [
                        SheepKey(end + 0.25, .at(0, 0, 4 * sign)),
                        SheepKey(end + 0.6, .rest),
                    ]
            }
            return t
        }

        func faceTracks() -> [SheepPart: Keys] {
            let first = Self.first
            let cheeks: Keys = [
                SheepKey(0, .rest), SheepKey(first, .rest),
                SheepKey(first + 0.3, .at(0, 0, 0, 1.2)),
                SheepKey(end, .at(0, 0, 0, 1.2)), SheepKey(end + 0.3, .rest),
            ]
            return [
                .mouth: [
                    SheepKey(0, .rest), SheepKey(first - 0.2, .rest, .out),
                    SheepKey(first + 0.1, .at(0, 0, 0, 1.25, 1.4)),
                    SheepKey(end, .at(0, 0, 0, 1.25, 1.4)),
                    SheepKey(end + 0.3, .rest),
                ],
                .eyes: [
                    SheepKey(0, .rest), SheepKey(first, .rest),
                    SheepKey(first + 0.15, .at(0, 3, 0, 1.05, 0.78)),
                    SheepKey(end, .at(0, 3, 0, 1.05, 0.78)),
                    SheepKey(end + 0.3, .rest),
                ],
                .cheekLeft: cheeks,
                .cheekRight: cheeks,
            ]
        }
    }
}
