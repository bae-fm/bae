import SwiftUI

/// Draws `content` at the current moment again as each whole second before
/// `deadline` passes. The timer is this view's own: core owns the deadline
/// and acts at it, and the view only counts down to it.
public struct CountdownTimeline<Content: View>: View {
    private let deadline: Date
    private let content: (Date) -> Content

    public init(
        deadline: Date,
        @ViewBuilder content: @escaping (_ now: Date) -> Content
    ) {
        self.deadline = deadline
        self.content = content
    }

    public var body: some View {
        TimelineView(.periodic(from: tickAnchor, by: 1)) { context in
            content(context.date)
        }
    }

    /// A day before the deadline, so ticks land on whole seconds before it
    /// and the anchor is in the past for any countdown core sends.
    private var tickAnchor: Date {
        deadline.addingTimeInterval(-86_400)
    }
}

/// Core's countdown message `key` with the whole seconds left from `now`
/// until `deadlineMs`, formatted against the current locale so the seconds
/// take its plural form.
public func localizedCountdownLine(key: String, deadlineMs: Int64, at now: Date)
    -> String
{
    localizedCoreString(
        key,
        countdownSecondsLeft(
            deadlineMs: deadlineMs,
            nowMs: now.millisecondsSince1970
        )
    )
}

/// Whole seconds from `nowMs` until `deadlineMs`, rounded up so a countdown
/// never reads 0 before its deadline, and never below 0.
public func countdownSecondsLeft(deadlineMs: Int64, nowMs: Int64) -> Int {
    let remainingMs = max(0, deadlineMs - nowMs)
    return Int((remainingMs + 999) / 1000)
}

extension Date {
    /// Milliseconds since 1970, rounded down.
    public var millisecondsSince1970: Int64 {
        Int64((timeIntervalSince1970 * 1000).rounded(.down))
    }
}
