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
        case .waitingForApproval:
            progress("Waiting for an admin to approve this device...")
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
