import SwiftUI

/// Draws `content` with core's countdown line again as each whole second
/// before the countdown's moment passes. The timer is this view's own: core
/// owns the moment and acts at it, and the view only counts down to it.
public struct CountdownTimeline<Content: View>: View {
    private let countdown: BridgeCountdown
    private let content: (String) -> Content

    public init(
        _ countdown: BridgeCountdown,
        @ViewBuilder content: @escaping (_ line: String) -> Content
    ) {
        self.countdown = countdown
        self.content = content
    }

    public var body: some View {
        TimelineView(.periodic(from: tickAnchor, by: 1)) { context in
            content(countdown.line(at: context.date))
        }
    }

    /// A day before the moment, so ticks land on whole seconds before it and
    /// the anchor is in the past for any countdown core sends.
    private var tickAnchor: Date {
        countdown.until.addingTimeInterval(-86_400)
    }
}

extension BridgeCountdown {
    /// The countdown's line at `now`: core's words with the whole seconds
    /// left, formatted against the current locale so the seconds take its
    /// plural form.
    public func line(at now: Date) -> String {
        localizedCoreString(key, secondsLeft(at: now))
    }

    /// Whole seconds from `now` until the moment, rounded up so the line
    /// never reads 0 before the moment, and never below 0.
    public func secondsLeft(at now: Date) -> Int {
        let remainingMs = max(
            0,
            until.millisecondsSince1970 - now.millisecondsSince1970
        )
        return Int((remainingMs + 999) / 1000)
    }
}

extension Date {
    /// Milliseconds since 1970, rounded down.
    var millisecondsSince1970: Int64 {
        Int64((timeIntervalSince1970 * 1000).rounded(.down))
    }
}
