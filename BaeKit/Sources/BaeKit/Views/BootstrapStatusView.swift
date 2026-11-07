import SwiftUI

/// What a running restore or join is doing, and anything coven is waiting on.
/// A step that counts, opening or loading, is drawn by the native line that
/// follows the call's steps, which stands in for this view while it runs.
public struct BootstrapStatusView: View {
    private let status: BridgeBootstrapStatus

    public init(status: BridgeBootstrapStatus) {
        self.status = status
    }

    public var body: some View {
        switch status {
        case .opening, .loading:
            // The step's own line takes over as soon as it draws.
            WaitingIndicator()
        case .connecting:
            progress("Connecting…")
        case .waitingForApproval(let confirmation, let nextCheck):
            VStack(spacing: ThemeSpace.related) {
                Text("Confirmation number")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                Text(verbatim: confirmation)
                    .themeText(.title)
                    .textSelection(.enabled)
                NextCheckLine(nextCheck: nextCheck)
            }
        case .registering:
            progress("Registering this device…")
        case .committing:
            progress("Finishing…")
        case .pending(let records):
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                progress("Waiting…")
                PendingRecordsView(records: records, perform: nil)
            }
        }
    }

    private func progress(_ title: LocalizedStringKey) -> some View {
        WaitingIndicator(title)
    }
}

/// When a waiting join checks for approval next, counting down each second.
struct NextCheckLine: View {
    let nextCheck: BridgeNextCheck

    var body: some View {
        switch nextCheck {
        case .checking:
            line(Text("Checking…"))
        case .at(let time):
            countdown(to: time)
        case .coolingDown(let until):
            countdown(to: until)
        }
    }

    private func countdown(to due: Date) -> some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let left = Self.secondsLeft(until: due, at: context.date)
            line(
                Text(
                    "Next check in \(Duration.seconds(left).formatted(.time(pattern: .minuteSecond)))"
                )
            )
        }
    }

    /// Whole seconds from `now` until `due`, rounded up so the line never reads
    /// 0 before the check, and never below 0.
    nonisolated static func secondsLeft(until due: Date, at now: Date) -> Int {
        max(0, Int(due.timeIntervalSince(now).rounded(.up)))
    }

    private func line(_ text: Text) -> some View {
        text
            .themeText(.body)
            .foregroundStyle(.secondary)
    }
}
