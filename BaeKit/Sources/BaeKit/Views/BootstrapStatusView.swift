import SwiftUI

/// What a running restore or join is doing, and anything coven is waiting on.
public struct BootstrapStatusView: View {
    private let status: BridgeBootstrapStatus

    public init(status: BridgeBootstrapStatus) {
        self.status = status
    }

    public var body: some View {
        switch status {
        case .connecting:
            progress("Connecting...")
        case .waitingForApproval(let confirmation, let nextCheck):
            VStack(spacing: ThemeSpace.related) {
                if let confirmation {
                    Text("Confirmation number")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                    Text(verbatim: confirmation)
                        .themeText(.title)
                        .textSelection(.enabled)
                }
                NextCheckLine(nextCheck: nextCheck)
            }
        case .loading:
            progress("Loading the library...")
        case .pending(let records):
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                progress("Waiting...")
                PendingRecordsView(
                    records: .ready(records: records),
                    perform: nil
                )
            }
        }
    }

    private func progress(_ title: LocalizedStringKey) -> some View {
        HStack(spacing: ThemeSpace.related) {
            ProgressView()
                .controlSize(.small)
            Text(title)
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
    }
}

/// When a waiting join checks for approval next, counting down each second.
private struct NextCheckLine: View {
    let nextCheck: BridgeNextCheck

    var body: some View {
        switch nextCheck {
        case .checking:
            line(Text("Checking..."))
        case .at(let time):
            countdown(to: time)
        case .coolingDown(let until):
            countdown(to: until)
        }
    }

    private func countdown(to due: Date) -> some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let left = max(
                0,
                Int(due.timeIntervalSince(context.date).rounded(.up))
            )
            line(
                Text(
                    "Next check in \(Duration.seconds(left).formatted(.time(pattern: .minuteSecond)))"
                )
            )
        }
    }

    private func line(_ text: Text) -> some View {
        text
            .themeText(.body)
            .foregroundStyle(.secondary)
    }
}
