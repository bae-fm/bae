import SwiftUI

public struct DeviceJoinProgressView: View {
    private enum Progress {
        case joining(BridgeJoiningDeviceJoinProgress)
        case admitting(BridgeAdmittingDeviceJoinProgress)
    }

    private let progress: Progress

    public init(joining progress: BridgeJoiningDeviceJoinProgress) {
        self.progress = .joining(progress)
    }

    public init(admitting progress: BridgeAdmittingDeviceJoinProgress) {
        self.progress = .admitting(progress)
    }

    public var body: some View {
        VStack(spacing: 8) {
            transferIndicator
            Text(title)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            if let bytes {
                Text(
                    verbatim:
                        "\(bytes.done.formattedDownloadBytes) / \(bytes.total.formattedDownloadBytes)"
                )
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var transferIndicator: some View {
        if let bytes, bytes.total > 0 {
            ProgressView(value: Double(bytes.done), total: Double(bytes.total))
                .progressViewStyle(.linear)
        }
        else {
            ProgressView()
        }
    }

    private var title: String {
        switch progress {
        case .joining(let value):
            localizedCoreString(
                bridgeJoiningDeviceJoinProgressKey(progress: value)
            )
        case .admitting(let value):
            localizedCoreString(
                bridgeAdmittingDeviceJoinProgressKey(progress: value)
            )
        }
    }

    private var bytes: (done: UInt64, total: UInt64)? {
        guard case .joining(let value) = progress else { return nil }
        switch value {
        case .downloadingSnapshot(let done, let total):
            return (done, total)
        default:
            return nil
        }
    }
}
