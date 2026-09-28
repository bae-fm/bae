import SwiftUI

/// A failing sync cycle as every app shows it: what failed, the fault, and a
/// way to reconnect when reconnecting can help.
public struct SyncFailureNotice: View {
    private let error: DisplayError
    private let canReconnect: Bool
    private let onReconnect: () async -> Void

    @State
    private var reconnecting = false

    public init(
        error: DisplayError,
        canReconnect: Bool,
        onReconnect: @escaping () async -> Void
    ) {
        self.error = error
        self.canReconnect = canReconnect
        self.onReconnect = onReconnect
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Label {
                Text("Sync is failing")
                    .themeText(.strong)
            } icon: {
                if let symbol = StatusTone.warning.symbol {
                    Image(systemName: symbol)
                        .foregroundStyle(StatusTone.warning.color)
                }
            }
            ErrorDetailDisclosure(error: error, tone: .neutral, showIcon: false)
            if canReconnect {
                HStack(spacing: ThemeSpace.related) {
                    Button("Reconnect") {
                        Task {
                            reconnecting = true
                            await onReconnect()
                            reconnecting = false
                        }
                    }
                    .disabled(reconnecting)
                    if reconnecting {
                        ProgressView()
                            .controlSize(.small)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(.warning)
    }
}
