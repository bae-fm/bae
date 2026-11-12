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
            MacSpinner()
        case .connecting:
            progress("Connecting…")
        case .waitingForApproval(let confirmation):
            VStack(spacing: ThemeSpace.related) {
                Text("Confirmation number")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                Text(verbatim: confirmation)
                    .themeText(.title)
                    .textSelection(.enabled)
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
        MacSpinner(title)
    }
}
