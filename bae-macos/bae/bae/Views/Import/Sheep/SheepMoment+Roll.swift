import SwiftUI

extension SheepMoment {
    /// The sheep tucks its head, springs up and rolls once on the spot, lands
    /// with a squash, and wobbles back upright. No release.
    static func roll() -> SheepMoment {
        SheepMoment(
            duration: 6,
            tracks: merged([Roll.bodyTracks(), Roll.faceTracks()])
        )
    }

    private enum Roll {
        static let go = 1.15
        static let down = 1.85

        static func bodyTracks() -> [SheepPart: Keys] {
            var t: [SheepPart: Keys] = [:]
            t[.roll] = [
                SheepKey(0, .rest), SheepKey(go, .rest, .out),
                SheepKey(go + 0.18, .at(0, -60, 70), .linear),
                SheepKey(go + 0.35, .at(0, -90, 180), .linear),
                SheepKey(go + 0.52, .at(0, -60, 290), .in),
                SheepKey(down, .at(0, 0, 360), .hold), SheepKey(6, .rest),
            ]
            t[.rig] = [
                SheepKey(0, .rest), SheepKey(0.7, .rest),
                SheepKey(go, .at(0, 0, 0, 1.1, 0.86), .snap),
                SheepKey(go + 0.12, .at(0, 0, 0, 0.94, 1.08), .out),
                SheepKey(go + 0.3, .at(0, 0, 0, 0.96, 0.96)),
                SheepKey(down - 0.05, .at(0, 0, 0, 0.98, 1.02), .in),
                SheepKey(down + 0.1, .at(0, 0, 0, 1.14, 0.84), .out),
                SheepKey(2.2, .at(0, 0, 0, 0.95, 1.06)),
                SheepKey(2.45, .at(0, 0, 0, 1.02, 0.98)), SheepKey(2.7, .rest),
            ]
            t[.arms] = [
                SheepKey(0, .rest), SheepKey(0.7, .rest),
                SheepKey(go, .at(0, -14, 0, 0.94), .snap),
                SheepKey(down - 0.05, .at(0, -14, 0, 0.94)),
                SheepKey(down + 0.15, .at(0, 10), .out), SheepKey(2.4, .rest),
            ]
            return t
        }

        static func faceTracks() -> [SheepPart: Keys] {
            var t: [SheepPart: Keys] = [:]
            t[.head] = [
                SheepKey(0, .rest), SheepKey(0.7, .rest),
                SheepKey(go, .at(0, 24, 0, 1.03, 0.95), .snap),
                SheepKey(go + 0.3, .at(0, 18, 0)),
                SheepKey(down - 0.05, .at(0, 10, 0), .in),
                SheepKey(down + 0.12, .at(0, 18, 0), .out),
                SheepKey(2.3, .at(0, -8, 0)), SheepKey(2.6, .at(0, 0, -5)),
                SheepKey(2.95, .at(0, 0, 4)), SheepKey(3.3, .at(0, 0, -2)),
                SheepKey(3.6, .rest),
            ]
            for (ear, s) in ears {
                t[ear] = [
                    SheepKey(0, .rest), SheepKey(0.7, .rest),
                    SheepKey(go, .at(0, 0, -16 * s), .snap),
                    SheepKey(go + 0.35, .at(0, 0, 24 * s), .linear),
                    SheepKey(down, .at(0, 0, -12 * s), .out),
                    SheepKey(down + 0.15, .at(0, 0, 28 * s), .out),
                    SheepKey(2.4, .at(0, 0, -10 * s)),
                    SheepKey(2.7, .at(0, 0, 6 * s)),
                    SheepKey(3.0, .at(0, 0, -2 * s)), SheepKey(3.3, .rest),
                ]
            }
            t[.eyes] = [
                SheepKey(0, .rest), SheepKey(0.8, .rest),
                SheepKey(go, .at(0, 4, 0, 1.08, 0.6)),
                SheepKey(down + 0.3, .at(0, 4, 0, 1.08, 0.6)),
                SheepKey(2.4, .rest),
            ]
            t[.mouth] = [
                SheepKey(0, .rest), SheepKey(down, .rest, .out),
                SheepKey(down + 0.2, .at(0, 0, 0, 1.3, 1.5)),
                SheepKey(3.2, .at(0, 0, 0, 1.25, 1.4)), SheepKey(3.5, .rest),
            ]
            let cheeks: Keys = [
                SheepKey(0, .rest), SheepKey(down, .rest, .back),
                SheepKey(down + 0.2, .at(0, 0, 0, 1.3)),
                SheepKey(3.2, .at(0, 0, 0, 1.25)), SheepKey(3.5, .rest),
            ]
            t[.cheekLeft] = cheeks
            t[.cheekRight] = cheeks
            return t
        }
    }
}
