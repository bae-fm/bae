import SwiftUI

struct QueueSectionHeaderStatus: Equatable {
    let pauseText: String?
    let summaryText: String?

    init(pauseStatusText: String?, summaryText: String) {
        pauseText = pauseStatusText
        self.summaryText = summaryText.isEmpty ? nil : summaryText
    }
}

/// Header band shared by the three queue panes: optional leading slot
/// (the outbox pane's collapse chevron), icon, title, pause status and queue
/// summary, pause/resume, and retry.
struct QueueSectionHeader<Leading: View>: View {
    let icon: String
    let title: LocalizedStringKey
    let pauseRequested: Bool
    let pauseStatusText: String?
    let summaryText: String
    let retryDisabled: Bool
    let onSetPaused: (Bool) -> Void
    let onRetry: () -> Void
    @ViewBuilder
    let leading: () -> Leading

    var body: some View {
        let status = QueueSectionHeaderStatus(
            pauseStatusText: pauseStatusText,
            summaryText: summaryText
        )
        HStack(spacing: 8) {
            leading()
            Image(systemName: icon)
                .foregroundStyle(.secondary)
            Text(title).font(.callout.weight(.medium))
            if let pauseText = status.pauseText {
                Label(pauseText, systemImage: "pause.circle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            if let summaryText = status.summaryText {
                Text(summaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(pauseRequested ? "Resume" : "Pause") {
                onSetPaused(!pauseRequested)
            }
            Button("Retry now", action: onRetry)
                .disabled(retryDisabled)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

extension QueueSectionHeader where Leading == EmptyView {
    init(
        icon: String,
        title: LocalizedStringKey,
        pauseRequested: Bool,
        pauseStatusText: String?,
        summaryText: String,
        retryDisabled: Bool,
        onSetPaused: @escaping (Bool) -> Void,
        onRetry: @escaping () -> Void
    ) {
        self.init(
            icon: icon,
            title: title,
            pauseRequested: pauseRequested,
            pauseStatusText: pauseStatusText,
            summaryText: summaryText,
            retryDisabled: retryDisabled,
            onSetPaused: onSetPaused,
            onRetry: onRetry
        ) { EmptyView() }
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

    #Preview("Paused, with leading slot") {
        QueueSectionHeader(
            icon: "arrow.up.arrow.down.circle",
            title: "Sync queue",
            pauseRequested: true,
            pauseStatusText: String(localized: "Paused"),
            summaryText: "",
            retryDisabled: false,
            onSetPaused: { _ in },
            onRetry: {},
            leading: {
                Image(systemName: "chevron.down")
                    .foregroundStyle(.secondary)
            }
        )
        .frame(width: 640)
    }
#endif
