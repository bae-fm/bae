import AppKit
import BaeKit
import SwiftUI

/// Bottom pane showing the cloud outbox processing queue: a master progress
/// row, a summary band with a "Retry now" action, and per-item lists for
/// uploads and deletes. Hidden when the queue is idle. The user can drag the
/// top edge to resize the item list (persisted) and collapse the pane to just
/// its header. Reads `OutboxStore` at the leaf; the outbox projection is the
/// sole writer, so actions don't optimistically mutate — an `.outbox`
/// invalidation refetches and refreshes the panel.
struct OutboxSection: View {
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    /// Persisted item-list height and collapsed state, so the pane keeps its
    /// size and visibility across sessions.
    @AppStorage("storageQueuePaneHeight")
    private var storedHeight: Double = 180
    @AppStorage("storageQueuePaneCollapsed")
    private var collapsed: Bool = false
    /// Live drag delta while the resize handle is held; resets to 0 on release.
    @GestureState
    private var dragOffset: CGFloat = 0

    /// Bounds for the resizable item list.
    private let heightRange: ClosedRange<CGFloat> = 80...480

    /// The item-list height with a drag `offset` applied, clamped to
    /// `heightRange`. Dragging the handle up (negative translation) grows the
    /// list; down shrinks it. Used for both the live height and the committed
    /// height so the two can't drift.
    private func paneHeight(applying offset: CGFloat) -> CGFloat {
        let height = CGFloat(storedHeight) - offset
        return min(max(height, heightRange.lowerBound), heightRange.upperBound)
    }

    var body: some View {
        let snapshot = outboxStore.snapshot
        if !snapshot.uploadGroups.isEmpty || !snapshot.deletes.isEmpty {
            Divider()
            VStack(spacing: 0) {
                if !collapsed {
                    resizeHandle
                }
                QueueSectionHeader(
                    icon: "arrow.up.arrow.down.circle",
                    title: "Sync queue",
                    paused: snapshot.paused,
                    summaryText: snapshot.summaryText,
                    retryDisabled: snapshot.total.failed == 0,
                    onSetPaused: { paused in
                        Task { try await sync.setSyncPaused(paused) }
                    },
                    onRetry: {
                        Task {
                            do { try await sync.retryOutbox() }
                            catch {
                                uiStore.showError(
                                    String(
                                        localized:
                                            "Failed to retry uploads: \(error.displayLine)"
                                    )
                                )
                            }
                        }
                    },
                    leading: { collapseButton }
                )
                if !collapsed {
                    if snapshot.total.bytesTotal > 0 {
                        OutboxTotalProgress(snapshot: snapshot)
                    }
                    Divider()
                    itemList(snapshot)
                        .frame(height: paneHeight(applying: dragOffset))
                }
            }
        }
    }

    /// A thin strip the user drags to resize the queue. Shows the resize cursor
    /// on hover; commits the new height on release.
    private var resizeHandle: some View {
        Color.clear
            .frame(height: 6)
            .overlay(Divider())
            .contentShape(Rectangle())
            .onHover { inside in
                if inside {
                    NSCursor.resizeUpDown.push()
                }
                else {
                    NSCursor.pop()
                }
            }
            .gesture(
                DragGesture()
                    .updating($dragOffset) { value, state, _ in
                        state = value.translation.height
                    }
                    .onEnded { value in
                        storedHeight = Double(
                            paneHeight(applying: value.translation.height)
                        )
                    }
            )
    }

    /// The collapse/expand chevron shown in the sync-queue header's leading
    /// slot; toggles the pane between its full body and just the header.
    private var collapseButton: some View {
        Button {
            collapsed.toggle()
        } label: {
            Image(systemName: collapsed ? "chevron.right" : "chevron.down")
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
        .help(
            collapsed ? "Expand the sync queue" : "Collapse the sync queue"
        )
    }

    private func itemList(_ snapshot: BridgeOutboxSnapshot) -> some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                // Uploads are grouped per release (matching the storage table);
                // right-click a release to cancel its transition.
                ForEach(snapshot.uploadGroups, id: \.releaseId) { group in
                    OutboxReleaseRow(group: group) {
                        if let releaseId = group.releaseId {
                            cancelTransition(releaseId)
                        }
                    }
                    Divider()
                }
                // Deletes stay per-file — a delete is a single-file operation.
                ForEach(snapshot.deletes, id: \.blobId) { op in
                    OutboxDeleteRow(op: op)
                    Divider()
                }
            }
        }
    }

    /// Cancel a release's in-progress transition, surfacing any failure.
    private func cancelTransition(_ releaseId: String) {
        Task {
            do { try await sync.cancelReleaseTransition(releaseId) }
            catch {
                uiStore.showError(
                    String(
                        localized:
                            "Failed to cancel: \(error.displayLine)"
                    )
                )
            }
        }
    }
}

#if DEBUG
    #Preview("Populated") {
        OutboxSection()
            .environment(PreviewData.outboxStore())
            .environment(Sync.stub)
            .environment(UiStore())
            .frame(width: 720, height: 360)
    }

    #Preview("Paused") {
        OutboxSection()
            .environment(
                PreviewData.outboxStore(
                    PreviewData.outboxSnapshot(paused: true)
                )
            )
            .environment(Sync.stub)
            .environment(UiStore())
            .frame(width: 720, height: 360)
    }
#endif
