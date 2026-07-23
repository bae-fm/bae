import SwiftUI

/// Header band shared by the three queue panes: optional leading slot
/// (the outbox pane's collapse chevron), icon, title, paused chip or
/// summary, pause/resume, and retry.
struct QueueSectionHeader<Leading: View>: View {
    let icon: String
    let title: LocalizedStringKey
    let paused: Bool
    let summaryText: String
    let retryDisabled: Bool
    let onSetPaused: (Bool) -> Void
    let onRetry: () -> Void
    @ViewBuilder
    let leading: () -> Leading

    var body: some View {
        HStack(spacing: 8) {
            leading()
            Image(systemName: icon)
                .foregroundStyle(.secondary)
            Text(title).font(.callout.weight(.medium))
            if paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            else if !summaryText.isEmpty {
                Text(summaryText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(paused ? "Resume" : "Pause") { onSetPaused(!paused) }
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
        paused: Bool,
        summaryText: String,
        retryDisabled: Bool,
        onSetPaused: @escaping (Bool) -> Void,
        onRetry: @escaping () -> Void
    ) {
        self.init(
            icon: icon,
            title: title,
            paused: paused,
            summaryText: summaryText,
            retryDisabled: retryDisabled,
            onSetPaused: onSetPaused,
            onRetry: onRetry
        ) { EmptyView() }
    }
}
