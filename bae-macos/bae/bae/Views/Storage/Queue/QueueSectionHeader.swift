import SwiftUI

struct QueueSectionHeaderStatus: Equatable {
    let pauseText: String?
    let summaryText: String?

    init(pauseStatusText: String?, summaryText: String) {
        pauseText = pauseStatusText
        self.summaryText = summaryText.isEmpty ? nil : summaryText
    }
}

/// Header band shared by the three queue-wide status sections. Status uses its
/// own line so the controls stay readable when the manager is narrow.
struct QueueSectionHeader: View {
    let icon: String
    let title: LocalizedStringKey
    let pauseRequested: Bool
    let pauseStatusText: String?
    let summaryText: String
    let retryDisabled: Bool
    let onSetPaused: (Bool) -> Void
    let onRetry: () -> Void
    var body: some View {
        let status = QueueSectionHeaderStatus(
            pauseStatusText: pauseStatusText,
            summaryText: summaryText
        )
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: icon)
                    .foregroundStyle(.secondary)
                Text(title).font(.callout.weight(.medium))
                if let pauseText = status.pauseText {
                    Label(pauseText, systemImage: "pause.circle.fill")
                        .font(.callout)
                        .foregroundStyle(.orange)
                }
                Spacer()
                Button(pauseRequested ? "Resume" : "Pause") {
                    onSetPaused(!pauseRequested)
                }
                Button("Retry now", action: onRetry)
                    .disabled(retryDisabled)
            }
            if let summaryText = status.summaryText {
                Text(summaryText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .controlSize(.small)
    }
}

#if DEBUG
    #Preview("Active") {
        QueueSectionHeader(
            icon: "arrow.down.circle",
            title: "Downloads",
            pauseRequested: false,
            pauseStatusText: nil,
            summaryText: "1 downloading · 2 queued",
            retryDisabled: true,
            onSetPaused: { _ in },
            onRetry: {}
        )
        .frame(width: 640)
    }

    #Preview("Paused") {
        QueueSectionHeader(
            icon: "arrow.up.arrow.down.circle",
            title: "Sync queue",
            pauseRequested: true,
            pauseStatusText: String(localized: "Paused"),
            summaryText: "",
            retryDisabled: false,
            onSetPaused: { _ in },
            onRetry: {}
        )
        .frame(width: 640)
    }
#endif
