import BaeKit
import SwiftUI

/// Compact queue-wide cloud status, progress, and controls. The summary keeps
/// pending deletes visible here because delete records do not carry a release
/// identity. Release-keyed upload details live in
/// `StorageTransferInspector`.
struct OutboxSection: View {
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        let snapshot = outboxStore.snapshot
        if !snapshot.uploadGroups.isEmpty || !snapshot.deletes.isEmpty {
            Divider()
            VStack(spacing: 0) {
                QueueSectionHeader(
                    icon: "arrow.up.arrow.down.circle",
                    title: "Sync queue",
                    pauseRequested: snapshot.pauseRequested,
                    pauseStatusText: Self.pauseStatusText(snapshot.pauseState),
                    summaryText: snapshot.summaryText,
                    retryDisabled: snapshot.total.retrying == 0,
                    onSetPaused: { paused in
                        Task {
                            await Self.setPaused(
                                paused,
                                sync: sync,
                                uiStore: uiStore
                            )
                        }
                    },
                    onRetry: {
                        Task {
                            do { try await sync.retryOutbox() }
                            catch {
                                uiStore.showError(error)
                            }
                        }
                    }
                )
                if snapshot.total.bar != nil {
                    OutboxTotalProgress(snapshot: snapshot)
                }
            }
        }
    }

    static func pauseStatusText(_ state: BridgeOutboxPauseState) -> String? {
        switch state {
        case .running:
            nil
        case .paused:
            String(localized: "Paused")
        }
    }

    /// Apply the absolute pause target and surface a bridge failure in this
    /// window instead of leaving the button press with no result.
    @MainActor
    static func setPaused(
        _ paused: Bool,
        sync: Sync,
        uiStore: UiStore
    ) async {
        do {
            try await sync.setSyncPaused(paused)
        }
        catch {
            uiStore.showError(error)
        }
    }
}

#if DEBUG
    #Preview("Populated") {
        OutboxSection()
            .environment(PreviewData.outboxStore())
            .environment(Sync.stub())
            .environment(UiStore())
            .frame(width: 720)
    }

    #Preview("Paused") {
        OutboxSection()
            .environment(
                PreviewData.outboxStore(
                    PreviewData.outboxSnapshot(pauseState: .paused)
                )
            )
            .environment(Sync.stub())
            .environment(UiStore())
            .frame(width: 720)
    }
#endif
