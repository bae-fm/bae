import SwiftUI

extension SheepMoment {
    /// A release spins down from above, from one side or straight above. The
    /// sheep startles and looks toward it, crouches, leaps and catches it
    /// into its player, lands with a squash and a burst of stars, blushes,
    /// and a heart floats up.
    static func catching(
        from side: SheepPart.Side,
        _ release: (kind: SheepRelease.Kind, tone: ReleaseTone)
    ) -> SheepMoment {
        let plan = Catch(side: side)
        return SheepMoment(
            duration: Catch.duration,
            releases: [
                SheepRelease(kind: release.kind, tone: release.tone, radius: 50)
            ],
            tracks: merged([
                plan.releaseTracks(), plan.bodyTracks(), plan.faceTracks(),
                plan.afterglowTracks(),
            ])
        )
    }

    private struct Catch {
        static let duration = 7.5
        static let lift = -120.0
        /// When the release is in the player, and when the sheep lands.
        static let caught = 1.95
        static let land = 2.32

        let side: SheepPart.Side

        /// 1 for a release from the right, -1 from the left, 0 from above.
        private var d: Double {
            switch side {
            case .right: 1
            case .left: -1
            case .top: 0
            }
        }

        /// A curve from where the release starts, bending at a far corner,
        /// to the player.
        private var route: (start: CGPoint, bend: CGPoint) {
            switch side {
            case .right: (CGPoint(x: 700, y: -220), CGPoint(x: 640, y: 330))
            case .left: (CGPoint(x: -160, y: -240), CGPoint(x: -200, y: 330))
            case .top: (CGPoint(x: 222, y: -300), CGPoint(x: 222, y: 0))
            }
        }

        private func fall(_ u: Double) -> SheepPose {
            let target = CGPoint(x: 222, y: 282)
            let w = pow(u, d != 0 ? 1.25 : 1.6)
            let a = (1 - w) * (1 - w)
            let b = 2 * (1 - w) * w
            let c = w * w
            return
                .at(
                    a * route.start.x + b * route.bend.x + c * target.x,
                    a * route.start.y + b * route.bend.y + c * target.y,
                    0,
                    0.8 + 0.2 * u
                )
                .opacity(min(1, u * 6))
        }

        func releaseTracks() -> [SheepPart: Keys] {
            let caught = Self.caught
            let gone = SheepPose.at(220, 290, 0, 0.05).opacity(0)
            let first = SheepPose.at(route.start.x, route.start.y, 0, 0.75)
            let turns = (d == 0 ? 0.5 : d) * -720
            return [
                .release(0):
                    [SheepKey(0, first.opacity(0), .linear)]
                    + along(0.55, caught - 0.05, every: 0.06, fall)
                    + [
                        SheepKey(caught + 0.05, .at(220, 288, 0, 0.35), .in),
                        SheepKey(caught + 0.12, gone, .linear),
                        SheepKey(Self.duration, gone, .linear),
                    ],
                .spin(0): [
                    SheepKey(0, .rest, .linear), SheepKey(0.55, .rest, .linear),
                    SheepKey(caught + 0.12, .at(0, 0, turns), .hold),
                    SheepKey(Self.duration, .rest, .linear),
                ],
            ]
        }

        func bodyTracks() -> [SheepPart: Keys] {
            let caught = Self.caught
            let land = Self.land
            var t: [SheepPart: Keys] = [:]
            t[.rig] = [
                SheepKey(0, .rest), SheepKey(0.85, .rest, .out),
                SheepKey(1.0, .at(0, -14, 0, 0.98, 1.03)),
                SheepKey(1.2, .rest, .out),
                SheepKey(1.45, .at(0, 0, 0, 1.1, 0.86), .snap),
                SheepKey(1.58, .at(0, Self.lift * 0.45, 0, 0.92, 1.1), .out),
                SheepKey(caught, .at(0, Self.lift), .in),
                SheepKey(land, .at(0, 0, 0, 0.97, 1.04), .out),
                SheepKey(land + 0.1, .at(0, 0, 0, 1.14, 0.84), .out),
                SheepKey(2.65, .at(0, 0, 0, 0.95, 1.06)),
                SheepKey(2.9, .at(0, 0, 0, 1.02, 0.98)),
                SheepKey(3.15, .rest),
            ]
            t[.arms] = [
                SheepKey(0, .rest), SheepKey(1.2, .rest),
                SheepKey(1.45, .at(0, 12), .snap),
                SheepKey(1.75, .at(0, -40, 0, 1.06), .out),
                SheepKey(caught, .at(0, -46, 0, 1.08)),
                SheepKey(caught + 0.12, .at(0, -30)),
                SheepKey(land + 0.1, .at(0, 14), .out),
                SheepKey(2.7, .at(0, -4)), SheepKey(3.0, .rest),
            ]
            t[.heart] = [
                SheepKey(0, .rest), SheepKey(caught + 0.05, .rest, .back),
                SheepKey(caught + 0.3, .at(0, 0, 0, 1.6)),
                SheepKey(2.7, .at(0, 0, 0, 1.1)),
                SheepKey(2.9, .at(0, 0, 0, 1.35)), SheepKey(3.3, .rest),
            ]
            t[.flash] = [
                SheepKey(0, .rest.opacity(0)),
                SheepKey(caught, .rest.opacity(0), .out),
                SheepKey(caught + 0.1, .rest.opacity(0.9)),
                SheepKey(2.6, .rest.opacity(0)),
            ]
            return t
        }

        func faceTracks() -> [SheepPart: Keys] {
            let caught = Self.caught
            let land = Self.land
            var t: [SheepPart: Keys] = [:]
            // It looks toward the release: up for one from straight above.
            t[.head] = [
                SheepKey(0, .rest), SheepKey(0.85, .rest, .out),
                SheepKey(1.0, .at(4 * d, d != 0 ? -10 : -16, 4 * d)),
                SheepKey(1.2, .at(4 * d, -4, 3 * d)),
                SheepKey(1.45, .at(0, 14, 0), .snap),
                SheepKey(1.6, .at(0, -6, 0), .out),
                SheepKey(caught, .at(0, -4, -2)),
                SheepKey(land, .at(0, -10, 0), .out),
                SheepKey(land + 0.12, .at(0, 16, 0)),
                SheepKey(2.75, .at(0, 0, -6)), SheepKey(3.05, .at(0, 0, 5)),
                SheepKey(3.35, .at(0, 0, -3)), SheepKey(3.65, .at(0, 0, 1.5)),
                SheepKey(3.95, .rest),
            ]
            t[.features] = [
                SheepKey(0, .rest), SheepKey(0.85, .rest, .out),
                SheepKey(1.0, .at(18 * d, d != 0 ? 0 : -10)),
                SheepKey(1.4, .at(18 * d, d != 0 ? 0 : -10)),
                SheepKey(1.6, .rest),
            ]
            for (ear, s) in ears {
                t[ear] = [
                    SheepKey(0, .rest), SheepKey(0.85, .rest, .out),
                    SheepKey(1.0, .at(0, 0, 30 * s)),
                    SheepKey(1.2, .at(0, 0, 22 * s)),
                    SheepKey(1.45, .at(0, 0, -10 * s), .snap),
                    SheepKey(1.62, .at(0, 0, -22 * s), .out),
                    SheepKey(caught, .at(0, 0, -8 * s)),
                    SheepKey(land + 0.1, .at(0, 0, 26 * s), .out),
                    SheepKey(2.7, .at(0, 0, -10 * s)),
                    SheepKey(3.0, .at(0, 0, 6 * s)),
                    SheepKey(3.3, .at(0, 0, -2 * s)), SheepKey(3.6, .rest),
                ]
            }
            t[.mouth] = [
                SheepKey(0, .rest), SheepKey(0.85, .rest, .out),
                SheepKey(1.0, .at(0, 0, 0, 0.8, 1.5)),
                SheepKey(1.4, .at(0, 0, 0, 0.8, 1.3)), SheepKey(1.6, .rest),
                SheepKey(land + 0.1, .rest, .out),
                SheepKey(2.6, .at(0, 0, 0, 1.35, 1.6)),
                SheepKey(3.6, .at(0, 0, 0, 1.3, 1.5)), SheepKey(4.0, .rest),
            ]
            let startled = SheepPose.at(0, 0, 0, 0.3).opacity(0)
            t[.surprise(side)] = [
                SheepKey(0, startled), SheepKey(0.86, startled, .out),
                SheepKey(1.0, .at(0, 0, 0, 1.1)), SheepKey(1.3, .rest),
                SheepKey(1.45, .at(0, 0, 0, 1.05).opacity(0)),
            ]
            return t
        }

        /// The cheeks and blush, the burst of stars and the heart that floats up.
        func afterglowTracks() -> [SheepPart: Keys] {
            let land = Self.land
            let cheeks: Keys = [
                SheepKey(0, .rest), SheepKey(land, .rest, .back),
                SheepKey(2.55, .at(0, 0, 0, 1.4)),
                SheepKey(3.9, .at(0, 0, 0, 1.35)), SheepKey(4.3, .rest),
            ]
            var t: [SheepPart: Keys] = [
                .cheekLeft: cheeks,
                .cheekRight: cheeks,
                .blush: [
                    SheepKey(0, .rest.opacity(0)),
                    SheepKey(land + 0.05, .rest.opacity(0)),
                    SheepKey(2.55, .rest), SheepKey(3.9, .rest),
                    SheepKey(4.3, .rest.opacity(0)),
                ],
                .floatingHeart:
                    [SheepKey(0, .at(300, 40, 0, 0.3).opacity(0))]
                    + along(2.5, 4.2, every: 0.1, Self.floating),
            ]
            for (index, lag) in [0.02, 0.1, 0.06, 0.16, 0.22].enumerated() {
                let start = land + lag
                t[.star(index)] = [
                    SheepKey(0, .at(0, 0, 0, 0)),
                    SheepKey(start, .at(0, 0, -40, 0), .back),
                    SheepKey(start + 0.3, .at(0, 0, 0, 1.15)),
                    SheepKey(start + 0.9, .at(0, -10, 20, 0.9), .in),
                    SheepKey(start + 1.4, .at(0, -14, 30, 0)),
                ]
            }
            return t
        }

        private static func floating(_ u: Double) -> SheepPose {
            let w = sin(u * .pi * 3)
            return
                .at(
                    300 + 26 * w,
                    40 - 210 * u,
                    10 * w,
                    0.7 + 0.3 * min(1, u * 5)
                )
                .opacity(min(1, u * 6) * (u < 0.6 ? 1 : (1 - u) / 0.4))
        }
    }
}
