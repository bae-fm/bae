import SwiftUI

extension SheepMoment {
    /// A line of three releases slides in from one side (`side` 1 from the
    /// right, -1 from the left) on a steady beat. The sheep turns its face to
    /// them, opens wide and chomps each as it arrives, munches with full
    /// cheeks, swallows, and the heart on its player pulses.
    static func gobble(
        from side: Double,
        _ releases: [(kind: SheepRelease.Kind, tone: ReleaseTone)]
    ) -> SheepMoment {
        let plan = Gobble(side: side)
        return SheepMoment(
            duration: Gobble.duration,
            releases: releases.map {
                SheepRelease(kind: $0.kind, tone: $0.tone, radius: 44)
            },
            tracks: merged([
                plan.releaseTracks(), plan.headTracks(), plan.mouthTracks(),
                plan.bodyTracks(),
            ])
        )
    }

    private struct Gobble {
        static let duration = 7.0
        static let beat = 0.36
        static let arrivals = [1.8, 1.8 + beat, 1.8 + 2 * beat]
        static let y = 300.0
        static let speed = 130 / beat

        /// 1 for releases from the right, -1 from the left.
        let side: Double

        private var last: Double { Self.arrivals[2] }
        private var startX: Double { side > 0 ? 820 : -236 }

        /// Where the releases enter the mouth, mirrored about the mouth's
        /// middle (x 226) for the left.
        private func mouthX(_ right: Double) -> Double {
            side > 0 ? right : 452 - right
        }

        func releaseTracks() -> [SheepPart: Keys] {
            var t: [SheepPart: Keys] = [:]
            let y = Self.y
            let x0 = startX
            for (index, arrival) in Self.arrivals.enumerated() {
                let travel = abs(x0 - mouthX(306)) / Self.speed
                let start = arrival - 0.06 - travel
                let far = SheepPose.at(x0, y).opacity(0)
                let gone = SheepPose.at(mouthX(276), y + 6, 0, 0.45).opacity(0)
                let entered = x0 - side * Self.speed * 0.15
                t[.release(index)] = [
                    SheepKey(0, far, .linear),
                    SheepKey(start, far, .linear),
                    SheepKey(start + 0.15, .at(entered, y), .linear),
                    SheepKey(arrival - 0.06, .at(mouthX(306), y), .linear),
                    SheepKey(
                        arrival - 0.01,
                        .at(mouthX(284), y + 4, 0, 0.62),
                        .linear
                    ),
                    SheepKey(arrival + 0.02, gone, .linear),
                    SheepKey(Self.duration, gone, .linear),
                ]
                // Each release sways in step with the beat on its way in.
                var sway: Keys = [SheepKey(0, .rest)]
                var step = 0
                var time = start
                while time < arrival {
                    let odd = step % 2 == 1
                    sway.append(
                        SheepKey(time, .at(0, odd ? -6 : 0, odd ? -7 : 7))
                    )
                    step += 1
                    time += Self.beat / 2
                }
                sway.append(SheepKey(arrival + 0.1, .rest))
                t[.spin(index)] = sway
            }
            return t
        }

        func headTracks() -> [SheepPart: Keys] {
            let d = side
            var head: Keys = [
                SheepKey(0, .rest), SheepKey(0.9, .rest),
                SheepKey(1.3, .at(6 * d, 0, 3 * d)),
                SheepKey(1.42, .at(6 * d, 0, 3 * d)),
                SheepKey(1.66, .at(-2 * d, -10, -2 * d), .snap),
            ]
            for arrival in Self.arrivals {
                head.append(SheepKey(arrival, .at(14 * d, 6, 4 * d), .out))
                head.append(SheepKey(arrival + 0.22, .at(4 * d, -5, 0), .snap))
            }
            head.removeLast()
            head += [
                SheepKey(2.85, .at(6 * d, 0, 2 * d)),
                SheepKey(3.05, .at(6 * d, 5, 2 * d)),
                SheepKey(3.3, .at(6 * d, -2, d)),
                SheepKey(3.55, .at(6 * d, 5, 2 * d)),
                SheepKey(3.8, .at(6 * d, -5, d), .in),
                SheepKey(3.98, .at(4 * d, 14, d), .out),
                SheepKey(4.3, .at(3 * d, -6, d)),
                SheepKey(4.75, .rest),
            ]
            var t: [SheepPart: Keys] = [.head: head]
            t[.features] = faceToward(
                d,
                on: (0.9, 1.3),
                off: (4.3, 4.75),
                shift: 34
            )
            for (ear, sign) in ears {
                t[ear] = earKeys(sign)
            }
            return t
        }

        private func earKeys(_ sign: Double) -> Keys {
            var keys: Keys = [
                SheepKey(0, .rest), SheepKey(0.9, .rest),
                SheepKey(1.3, .at(0, 0, 8 * sign), .out),
            ]
            for arrival in Self.arrivals {
                keys.append(
                    SheepKey(arrival + 0.06, .at(0, 0, -6 * sign), .out)
                )
                keys.append(SheepKey(arrival + 0.28, .at(0, 0, 6 * sign)))
            }
            return keys + [
                SheepKey(3.0, .at(0, 0, 4 * sign)),
                SheepKey(4.05, .at(0, 0, -8 * sign), .out),
                SheepKey(4.35, .at(0, 0, 22 * sign)),
                SheepKey(4.65, .at(0, 0, -6 * sign)),
                SheepKey(4.95, .at(0, 0, 2 * sign)),
                SheepKey(5.25, .rest),
            ]
        }

        func mouthTracks() -> [SheepPart: Keys] {
            let shut = SheepPose.at(0, -2, 0, 0.85, 0.1)
            var gape: Keys = [
                SheepKey(0, shut.opacity(0)),
                SheepKey(1.42, shut.opacity(0), .linear),
                SheepKey(1.48, shut, .out),
                SheepKey(1.68, .rest, .snap),
            ]
            for arrival in Self.arrivals {
                gape.append(SheepKey(arrival + 0.05, shut, .out))
                if arrival != last {
                    gape.append(SheepKey(arrival + 0.25, .rest, .snap))
                }
            }
            gape.append(SheepKey(last + 0.06, shut, .linear))
            gape.append(SheepKey(last + 0.07, shut.opacity(0)))
            return [
                .mouth: [
                    SheepKey(0, .rest), SheepKey(1.42, .rest, .linear),
                    SheepKey(1.5, .rest.opacity(0)),
                    SheepKey(4.24, .rest.opacity(0), .linear),
                    SheepKey(4.32, .rest),
                ],
                .gape: gape,
                .munch: munching(
                    from: last + 0.07,
                    chews: [3.05, 3.55],
                    until: 4.22
                ),
            ]
        }

        func bodyTracks() -> [SheepPart: Keys] {
            let near: SheepPart = side > 0 ? .cheekRight : .cheekLeft
            let far: SheepPart = side > 0 ? .cheekLeft : .cheekRight
            var t: [SheepPart: Keys] = [:]
            for (cheek, width) in [(far, 1.1), (near, 0.8)] {
                t[cheek] = [
                    SheepKey(0, .rest), SheepKey(0.9, .rest),
                    SheepKey(1.3, .at(0, 0, 0, width, 1)),
                    SheepKey(last, .at(0, 0, 0, width, 1), .back),
                    SheepKey(2.75, .at(0, 0, 0, width * 1.45, 1.45)),
                    SheepKey(3.85, .at(0, 0, 0, width * 1.4, 1.4), .out),
                    SheepKey(4.1, .at(0, 0, 0, width, 1)),
                    SheepKey(4.3, .at(0, 0, 0, width, 1)),
                    SheepKey(4.75, .rest),
                ]
            }
            t[.eyes] = [
                SheepKey(0, .rest), SheepKey(2.6, .rest),
                SheepKey(2.8, .at(0, 3, 0, 1.06, 0.72)),
                SheepKey(3.9, .at(0, 3, 0, 1.06, 0.72), .out),
                SheepKey(4.2, .rest),
            ]
            var rig: Keys = [
                SheepKey(0, .rest), SheepKey(1.42, .rest),
                SheepKey(1.66, .at(0, 0, 0, 0.98, 1.03), .snap),
            ]
            for arrival in Self.arrivals {
                rig.append(SheepKey(arrival, .at(0, 0, 0, 1.03, 0.97), .out))
                rig.append(
                    SheepKey(arrival + 0.22, .at(0, 0, 0, 0.995, 1.005), .snap)
                )
            }
            rig.removeLast()
            t[.rig] = rig + swallow(at: 3.8)
            t[.heart] = [
                SheepKey(0, .rest), SheepKey(4.05, .rest, .back),
                SheepKey(4.3, .at(0, 0, 0, 1.5)), SheepKey(4.8, .rest),
            ]
            return t
        }
    }
}
