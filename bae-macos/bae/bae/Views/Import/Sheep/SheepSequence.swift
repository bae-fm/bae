import SwiftUI

/// The sheep's loop: eight moments one after another, then round again. No
/// moment follows itself, and each brings different releases. The loop runs
/// 57 seconds.
enum SheepSequence {
    static let moments: [SheepMoment] = [
        .gobble(
            from: 1,
            [(.cover, .blue), (.vinyl, .gold), (.cd, .pink)]
        ),
        .catching(from: .left, (.vinyl, .sky)),
        .bop(),
        .gobble(
            from: -1,
            [(.cd, .mint), (.cover, .coral), (.vinyl, .orchid)]
        ),
        .inhale(from: 1, (.sleeve(disc: .blue), .lime)),
        .catching(from: .top, (.cd, .coral)),
        .roll(),
        .catching(from: .right, (.cover, .orchid)),
    ]

    static let duration = moments.reduce(0) { $0 + $1.duration }

    /// The moment playing `seconds` into the loop and how far into it that is.
    static func moment(at seconds: Double) -> (
        moment: SheepMoment, time: Double
    ) {
        var remaining = seconds.truncatingRemainder(dividingBy: duration)
        for moment in moments {
            if remaining < moment.duration { return (moment, remaining) }
            remaining -= moment.duration
        }
        return (moments[moments.count - 1], moments[moments.count - 1].duration)
    }

    /// The sheep held still, as reduced motion shows it: at rest, with the
    /// sequence's first release, a blue cover, beside it.
    static let still = SheepMoment(
        duration: 1,
        releases: [SheepRelease(kind: .cover, tone: .blue, radius: 44)],
        tracks: [.release(0): [SheepKey(0, .at(588, 482, 10))]]
    )
}
