import SwiftUI

extension SheepMoment {
    /// The sheep turns to one side (`side` 1 right, -1 left), opens its mouth
    /// round and breathes in; streaks of air rush toward it and a release far
    /// off wobbles, then is pulled in, faster as it nears, and vanishes into
    /// the mouth. Cheeks and head swell full, it holds, then gulps.
    static func inhale(
        from side: Double,
        _ release: (kind: SheepRelease.Kind, tone: ReleaseTone)
    ) -> SheepMoment {
        let plan = Inhale(side: side)
        return SheepMoment(
            duration: Inhale.duration,
            releases: [
                SheepRelease(kind: release.kind, tone: release.tone, radius: 54)
            ],
            tracks: merged([
                plan.faceTracks(), plan.bodyTracks(), plan.releaseTracks(),
                plan.airTracks(),
            ])
        )
    }

    private struct Inhale {
        static let duration = 7.5
        /// When the release goes in.
        static let got = 2.7

        /// 1 for a breath to the right, -1 to the left.
        let side: Double

        private var mouthX: Double { 226 + 30 * side }
        private var startX: Double { side > 0 ? 660 : -170 + 70 }

        func faceTracks() -> [SheepPart: Keys] {
            let d = side
            let got = Self.got
            var t: [SheepPart: Keys] = [:]
            t[.features] = faceToward(
                d,
                on: (0.6, 1.0),
                off: (4.2, 4.7),
                shift: 30
            )
            t[.head] = [
                SheepKey(0, .rest), SheepKey(0.6, .rest),
                SheepKey(1.0, .at(4 * d, 0, 2 * d)),
                SheepKey(1.25, .at(-10 * d, -6, -3 * d), .out),
                SheepKey(got - 0.1, .at(-14 * d, -8, -4 * d), .out),
                SheepKey(got + 0.08, .at(6 * d, 4, 2 * d, 1.07, 1.05), .out),
                SheepKey(3.0, .at(4 * d, 0, d, 1.06, 1.05)),
                SheepKey(3.3, .at(4 * d, -2, d, 1.07, 1.04)),
                SheepKey(3.55, .at(4 * d, 0, d, 1.06, 1.05), .in),
                SheepKey(3.75, .at(2 * d, 12, 0), .out),
                SheepKey(4.05, .at(0, -5, 0)), SheepKey(4.4, .rest),
            ]
            // The mouth opens round (the wide mouth, drawn smaller), widens as
            // it pulls, and shuts on the release.
            let small = SheepPose.at(0, 0, 0, 0.3, 0.1)
            t[.mouth] = [
                SheepKey(0, .rest), SheepKey(1.0, .rest, .linear),
                SheepKey(1.06, .rest.opacity(0)),
                SheepKey(3.97, .rest.opacity(0), .linear),
                SheepKey(4.05, .rest),
            ]
            t[.gape] = [
                SheepKey(0, small.opacity(0)),
                SheepKey(1.0, small.opacity(0), .linear),
                SheepKey(1.04, small, .out),
                SheepKey(1.3, .at(0, 0, 0, 0.52, 0.62)),
                SheepKey(got - 0.2, .at(0, 0, 0, 0.6, 0.78)),
                SheepKey(got, .at(0, 0, 0, 0.5, 0.1), .linear),
                SheepKey(got + 0.03, small.opacity(0)),
            ]
            t[.munch] = munching(from: got + 0.03, chews: [], until: 3.97)
            t[.eyes] = [
                SheepKey(0, .rest), SheepKey(1.2, .rest),
                SheepKey(1.4, .at(0, 2, 0, 1.04, 0.82)),
                SheepKey(got, .at(0, 2, 0, 1.04, 0.82)),
                SheepKey(got + 0.1, .at(0, 4, 0, 1.1, 0.6)),
                SheepKey(3.7, .at(0, 4, 0, 1.1, 0.6)), SheepKey(4.0, .rest),
            ]
            let cheeks: Keys = [
                SheepKey(0, .rest), SheepKey(got, .rest, .back),
                SheepKey(got + 0.2, .at(0, 0, 0, 1.9, 1.75)),
                SheepKey(3.6, .at(0, 0, 0, 1.8, 1.7), .out),
                SheepKey(3.85, .rest),
            ]
            t[.cheekLeft] = cheeks
            t[.cheekRight] = cheeks
            return t
        }

        func bodyTracks() -> [SheepPart: Keys] {
            let got = Self.got
            var t: [SheepPart: Keys] = [:]
            t[.rig] =
                [
                    SheepKey(0, .rest), SheepKey(1.0, .rest),
                    SheepKey(1.6, .at(0, -4, 0, 1.02, 1.03)),
                    SheepKey(got, .at(0, -6, 0, 1.03, 1.04)),
                    SheepKey(got + 0.12, .at(0, 0, 0, 1.06, 0.97), .out),
                    SheepKey(3.55, .at(0, 0, 0, 1.05, 0.98)),
                ] + swallow(at: 3.75).dropFirst()
            for (ear, s) in ears {
                // The ears stream toward the pull, then flick after the gulp.
                let toward = side * s > 0 ? -1.0 : 1.0
                t[ear] = [
                    SheepKey(0, .rest), SheepKey(1.2, .rest),
                    SheepKey(1.6, .at(0, 0, 10 * toward * s)),
                    SheepKey(got, .at(0, 0, 14 * toward * s)),
                    SheepKey(got + 0.15, .at(0, 0, 12 * s), .out),
                    SheepKey(3.75, .at(0, 0, -6 * s), .out),
                    SheepKey(4.05, .at(0, 0, 18 * s)),
                    SheepKey(4.35, .at(0, 0, -5 * s)), SheepKey(4.65, .rest),
                ]
            }
            t[.heart] = [
                SheepKey(0, .rest), SheepKey(3.95, .rest, .back),
                SheepKey(4.2, .at(0, 0, 0, 1.5)), SheepKey(4.7, .rest),
            ]
            return t
        }

        /// The release: still far off, it wobbles against the pull, then
        /// comes in faster and faster, shrinking and turning.
        func releaseTracks() -> [SheepPart: Keys] {
            let d = side
            let x0 = startX
            let got = Self.got
            let gone = SheepPose.at(mouthX, 304, 0, 0.2).opacity(0)
            var wobble: Keys = [
                SheepKey(0, .at(x0, 290, 0).opacity(0), .linear),
                SheepKey(1.2, .at(x0, 290, 0).opacity(0), .out),
                SheepKey(1.45, .at(x0, 290, -8 * d)),
            ]
            for (index, time) in [1.65, 1.85].enumerated() {
                let turn = (index % 2 == 1 ? -10.0 : 6) * d
                let drift = 8 + 10 * Double(index)
                wobble.append(SheepKey(time, .at(x0 - d * drift, 290, turn)))
            }
            let pull = along(2.0, got - 0.02, every: 0.07) { [mouthX] u in
                let w = pow(u, 2.4)
                return .at(
                    x0 - d * 18 + (mouthX - x0 + d * 18) * w,
                    290 + 14 * w,
                    -d * 300 * w,
                    1 - 0.65 * w
                )
            }
            return [
                .release(0):
                    wobble + pull + [
                        SheepKey(got + 0.02, gone, .linear),
                        SheepKey(Self.duration, gone, .linear),
                    ],
                .spin(0): [SheepKey(0, .rest), SheepKey(Self.duration, .rest)],
            ]
        }

        /// Four streaks of air, staggered, each rushing from beside the
        /// release to the mouth twice over.
        func airTracks() -> [SheepPart: Keys] {
            let d = side
            let x0 = startX
            var t: [SheepPart: Keys] = [:]
            let streaks = [
                (0.0, 300.0), (0.12, 245.0), (0.24, 350.0), (0.36, 275.0),
            ]
            for (index, streak) in streaks.enumerated() {
                let (delay, y) = streak
                var keys: Keys = [
                    SheepKey(0, .at(x0, y, 0, 1, 1).opacity(0), .linear)
                ]
                for first in [1.3, 1.95] {
                    let start = first + delay
                    keys += [
                        SheepKey(
                            start,
                            .at(x0 - d * 30, y, 0, 0.6, 1).opacity(0),
                            .in
                        ),
                        SheepKey(
                            start + 0.12,
                            .at(x0 - d * 90, y + (300 - y) * 0.2, 0, 1, 1),
                            .in
                        ),
                        SheepKey(
                            start + 0.5,
                            .at(
                                mouthX + d * 70,
                                300 + (y - 300) * 0.25,
                                0,
                                0.5,
                                1
                            )
                            .opacity(0),
                            .linear
                        ),
                    ]
                }
                t[.wind(index)] = keys
            }
            return t
        }
    }
}
