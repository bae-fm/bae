import BaeKit
import SwiftUI
import UniformTypeIdentifiers

/// Range fetched around an unloaded context row as the queue scrolls, mirroring
/// the page size `PlaybackStore.loadUpcomingRange`'s bridge call uses.
private let queueUpcomingLoadBatchSize = 100

struct QueueView: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue

    let isActive: Bool
    let nowPlayingTitle: String?
    let nowPlayingArtist: String?
    let nowPlayingCover: ImageContent?
    let onClose: () -> Void
    let onClear: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    /// Move the entry `entryId` to sit before `beforeEntryId`; `nil` moves it
    /// to the end of its lane.
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Flip the playing context between sequential and shuffled order. Wired only
    /// to the context section's header — shuffle is a property of the context.
    let onSetShuffle: (Bool) -> Void

    // A drag can start in either section; the dragged entry id is shared so the
    // source row dims wherever it lives. Hover and drop-insertion are positional,
    // so each QueueSection owns its own — a row index in one lane must not light
    // up the same index in the other.
    @State
    private var draggedEntryId: String?

    /// The manual lane ("Up Next"): explicitly enqueued tracks, drained first —
    /// always resolved in full, never windowed.
    private var manual: [QueueItem] { playbackStore.manualQueue }
    /// The context (the release being played from), or `nil` when nothing plays
    /// from a release. Rendered as a section distinct from the manual lane;
    /// `upcomingTotal` may exceed what's currently loaded.
    private var context: QueuePlaybackContext? { playbackStore.queueContext }

    private var isEmpty: Bool {
        // No context (nothing playing from a release) contributes no rows — a
        // normal state, named here rather than folded into a default.
        switch context {
        case .none:
            return manual.isEmpty
        case .some(let context):
            return manual.isEmpty && context.upcomingTotal == 0
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()

            if isActive {
                nowPlayingSection
                Divider()
            }

            if isEmpty {
                ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: Text("Drag tracks here or play an album"),
                )
                .frame(maxHeight: .infinity)
                .dropDestination(for: String.self) { droppedIds, _ in
                    // A dragged album card may carry several ids joined by a
                    // newline (a multi-selection drag); split each back out.
                    let ids = droppedIds.flatMap(AlbumDragPayload.decode)
                    guard !ids.isEmpty else {
                        return false
                    }
                    onInsertTracks(ids, 0)
                    return true
                }
            }
            else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        // The manual lane drains first, so it is shown first. It
                        // accepts external track drops (Play Next / Add to Queue
                        // land here); the context section, being the release's own
                        // order, takes reorder/remove/skip only. It is always
                        // fully resolved, so it never has a load hook or unloaded
                        // rows.
                        QueueSection(
                            title: String(localized: "Up Next"),
                            shuffled: false,
                            count: manual.count,
                            itemAt: { index in
                                manual.indices.contains(index)
                                    ? manual[index] : nil
                            },
                            loadEpoch: 0,
                            loadRange: nil,
                            acceptsExternalDrops: true,
                            draggedEntryId: $draggedEntryId,
                            onSkipTo: onSkipTo,
                            onRemove: onRemove,
                            onReorder: onReorder,
                            onInsertTracks: onInsertTracks,
                            onSetShuffle: nil,
                        )

                        if let context, context.upcomingTotal > 0 {
                            // The context tail is library-scaled and only
                            // partly resolved (`upcomingItem` returns `nil` for
                            // an index not yet loaded); the section renders a
                            // placeholder for those rows and fetches the range
                            // around them.
                            QueueSection(
                                title: Self.contextSectionTitle(context.kind),
                                shuffled: context.shuffled,
                                count: context.upcomingTotal,
                                itemAt: { playbackStore.upcomingItem(at: $0) },
                                loadEpoch: playbackStore.revision,
                                loadRange: { offset, limit in
                                    await playbackStore.loadUpcomingRange(
                                        offset: offset,
                                        limit: limit,
                                        queue: queue
                                    )
                                },
                                acceptsExternalDrops: false,
                                draggedEntryId: $draggedEntryId,
                                onSkipTo: onSkipTo,
                                onRemove: onRemove,
                                onReorder: onReorder,
                                onInsertTracks: onInsertTracks,
                                onSetShuffle: onSetShuffle,
                            )
                        }
                    }
                }
                .background(Theme.background)
            }
        }
        .background(Theme.surface)
    }

    /// The context section's title, by what it plays from: a release keeps the
    /// "Playing From" label; the library names itself. Resolving a localized key
    /// by the source kind is the UI's locale-rendering job — the kind crosses the
    /// bridge as an enum, the prose stays here.
    private static func contextSectionTitle(_ kind: BridgePlaybackSourceKind)
        -> String
    {
        switch kind {
        case .release:
            return String(localized: "Playing From")
        case .library:
            return String(localized: "Your Library")
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            Text("Queue")
                .font(.headline)
            Spacer()
            // Clear empties only the manual lane; the context (the release being
            // played from) survives, so the control disables on an empty manual
            // lane regardless of the context.
            Button("Clear") { onClear() }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(manual.isEmpty)
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
        }
        .padding()
    }

    // MARK: - Now Playing

    private var nowPlayingSection: some View {
        HStack(spacing: 12) {
            nowPlayingArt
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))

            VStack(alignment: .leading, spacing: 2) {
                Text("Now Playing")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                if let title = nowPlayingTitle {
                    Text(title)
                        .font(.callout)
                        .fontWeight(.medium)
                        .lineLimit(1)
                }
                if let artist = nowPlayingArtist {
                    Text(artist)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer()
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private var nowPlayingArt: some View {
        ImageView(content: nowPlayingCover, pointSize: 48)
    }
}

// MARK: - Section

// periphery:ignore
/// Row load identity: which section epoch (the store's queue revision, so a
/// bump restarts every visible row's load task) and which absolute index. The
/// manual lane, always fully loaded, passes a constant epoch and a `nil` load
/// hook so its rows never fire a load.
private struct QueueRowLoadID: Hashable {
    let epoch: UInt64
    let index: Int
}

/// One lane of the queue (the manual "Up Next" lane or the context), rendered as
/// a labelled section of reorderable rows addressed by absolute index —
/// `itemAt(index)` resolves the loaded row, or `nil` for a not-yet-loaded
/// context row, which renders a placeholder and triggers `loadRange`. Only the
/// manual lane sets `acceptsExternalDrops` (external track drops land in the
/// manual lane, never the release's own order). Hover and drop-insertion are
/// positional, so each section owns its own state.
private struct QueueSection: View {
    let title: String
    let shuffled: Bool
    let count: Int
    let itemAt: (Int) -> QueueItem?
    /// The store's queue revision — folded into each row's `.task(id:)` so a
    /// queue change restarts in-flight loads rather than resolving into a
    /// superseded window. Unused (fixed at 0) for the always-loaded manual lane.
    let loadEpoch: UInt64
    /// Fetch `[offset, offset + limit)` and merge it into the store. `nil` for
    /// the manual lane, which is never windowed.
    let loadRange: ((_ offset: Int, _ limit: Int) async -> Void)?
    let acceptsExternalDrops: Bool
    @Binding
    var draggedEntryId: String?
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Flip this section between sequential and shuffled order, given its current
    /// `shuffled` state. `nil` on the manual lane, which has no shuffle control.
    let onSetShuffle: ((Bool) -> Void)?

    @State
    private var hoveredIndex: Int?
    @State
    private var dropInsertIndex: Int?

    var body: some View {
        VStack(spacing: 0) {
            sectionHeader

            ForEach(0..<count, id: \.self) { index in
                let item = itemAt(index)
                VStack(spacing: 0) {
                    // Kept in the tree and toggled by opacity so showing the drop
                    // line doesn't change a row's size and re-lay-out the lane.
                    insertionLine
                        .opacity(dropInsertIndex == index ? 1 : 0)
                        .allowsHitTesting(false)
                    queueRow(item, index: index)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 4)
                        .opacity(
                            item != nil && draggedEntryId == item?.id
                                ? 0.3 : 1.0
                        )
                    Divider().padding(.leading, 62)
                }
                .task(id: QueueRowLoadID(epoch: loadEpoch, index: index)) {
                    guard item == nil, let loadRange else {
                        return
                    }
                    let first = max(0, index - queueUpcomingLoadBatchSize / 2)
                    let end = min(
                        first + queueUpcomingLoadBatchSize,
                        count
                    )
                    await loadRange(first, end - first)
                }
                .onDrop(
                    of: [UTType.plainText],
                    delegate: QueueDropDelegate(
                        targetIndex: index,
                        targetLoaded: item != nil,
                        count: count,
                        itemAt: itemAt,
                        acceptsExternalDrops: acceptsExternalDrops,
                        draggedEntryId: $draggedEntryId,
                        dropInsertIndex: $dropInsertIndex,
                        onReorder: onReorder,
                        onInsertTracks: onInsertTracks,
                    )
                )
            }
            // The trailing drop line (insert at the lane's end) stays in the tree
            // and toggles by opacity, matching the per-row lines above.
            insertionLine
                .opacity(dropInsertIndex == count ? 1 : 0)
                .allowsHitTesting(false)

            // Trailing drop zone for appending — only the manual lane appends
            // external tracks; the context still accepts a reorder-to-end here.
            Color.clear
                .frame(height: acceptsExternalDrops ? 40 : 12)
                .onDrop(
                    of: [UTType.plainText],
                    delegate: QueueDropDelegate(
                        targetIndex: count,
                        targetLoaded: true,
                        count: count,
                        itemAt: itemAt,
                        acceptsExternalDrops: acceptsExternalDrops,
                        draggedEntryId: $draggedEntryId,
                        dropInsertIndex: $dropInsertIndex,
                        onReorder: onReorder,
                        onInsertTracks: onInsertTracks,
                    )
                )
        }
    }

    private var sectionHeader: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.caption)
                .fontWeight(.semibold)
                .foregroundStyle(.secondary)
            Spacer()
            if let onSetShuffle {
                shuffleToggle(onSetShuffle)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 4)
    }

    /// The context's shuffle toggle: tinted when on, muted when off; tapping it
    /// flips the context's order while the current track keeps playing.
    private func shuffleToggle(_ onSetShuffle: @escaping (Bool) -> Void)
        -> some View
    {
        Button {
            onSetShuffle(!shuffled)
        } label: {
            Image(systemName: "shuffle")
                .font(.caption2)
                .foregroundStyle(shuffled ? Color.accentColor : .secondary)
        }
        .buttonStyle(.plain)
        .help(shuffled ? "Turn off shuffle" : "Shuffle")
        .accessibilityLabel(shuffled ? "Turn off shuffle" : "Shuffle")
    }

    private var insertionLine: some View {
        Rectangle()
            .fill(Color.accentColor)
            .frame(height: 2)
            .padding(.horizontal, 8)
    }

    // The hover play overlay stays in the tree and toggles by opacity/hit-testing
    // so revealing it on hover doesn't resize the row and re-lay-out the lane.
    private func queueItemArtWithHover(_ item: QueueItem, index: Int)
        -> some View
    {
        let isHovered = hoveredIndex == index
        return ZStack {
            queueItemArt(coverImageId: item.coverImageId)
                .frame(width: 40, height: 40)
                .clipShape(RoundedRectangle(cornerRadius: 3))

            RoundedRectangle(cornerRadius: 3)
                .fill(.black.opacity(0.5))
                .frame(width: 40, height: 40)
                .opacity(isHovered ? 1 : 0)
            Button(action: { onSkipTo(item.id) }) {
                Image(systemName: "play.fill")
                    .font(.caption)
                    .foregroundColor(.white)
            }
            .buttonStyle(.plain)
            .opacity(isHovered ? 1 : 0)
            .allowsHitTesting(isHovered)
        }
    }

    /// A not-yet-loaded row: a skeleton shape, no text — `loadRange` is already
    /// in flight for it via the row's `.task(id:)`.
    private func queuePlaceholderRow() -> some View {
        HStack(spacing: 10) {
            RoundedRectangle(cornerRadius: 3)
                .fill(.secondary.opacity(0.15))
                .frame(width: 40, height: 40)
            VStack(alignment: .leading, spacing: 4) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 140, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 90, height: 10)
            }
            Spacer()
        }
    }

    /// A row at `index`: the resolved item, or a placeholder while it loads.
    private func queueRow(_ item: QueueItem?, index: Int) -> some View {
        Group {
            if let item {
                queueItemRow(item, index: index)
            }
            else {
                queuePlaceholderRow()
            }
        }
    }

    private func queueItemRow(_ item: QueueItem, index: Int) -> some View {
        HStack(spacing: 10) {
            queueItemArtWithHover(item, index: index)

            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.callout)
                    .lineLimit(1)
                Text(item.albumTitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            // Hover swaps the duration for a remove button. Both stay in the tree
            // and toggle by opacity/hit-testing so the swap doesn't resize the row
            // and re-lay-out the lane. (The trailing slot is the wider of the two,
            // so neither toggle changes the row's intrinsic width.)
            let isHovered = hoveredIndex == index
            ZStack(alignment: .trailing) {
                Text(item.durationLabel)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .opacity(isHovered ? 0 : 1)
                Button(action: { onRemove(item.id) }) {
                    Image(systemName: "xmark")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help("Remove from queue")
                .opacity(isHovered ? 1 : 0)
                .allowsHitTesting(isHovered)
            }
        }
        .contentShape(Rectangle())
        .onHover { isHovered in
            hoveredIndex = isHovered ? index : nil
        }
        .onDrag {
            draggedEntryId = item.id
            return NSItemProvider(object: item.id as NSString)
        }
        .onTapGesture(count: 2) {
            onSkipTo(item.id)
        }
        .contextMenu {
            Button("Remove from Queue") {
                onRemove(item.id)
            }
        }
    }

    private func queueItemArt(coverImageId: String?) -> some View {
        ImageView(coverImageId: coverImageId, pointSize: 40)
    }
}

// MARK: - Drop Delegate

private struct QueueDropDelegate: DropDelegate {
    let targetIndex: Int
    /// Whether `itemAt(targetIndex)` is loaded. An internal reorder onto an
    /// unloaded row is rejected — `onReorder` needs the target's own entry id
    /// (core reorders by id, not index), which isn't known yet. In practice a
    /// row must be visible (and its load task already running) to receive a
    /// drop at all, so this only ever excludes a brief loading flash, not a
    /// real position.
    let targetLoaded: Bool
    let count: Int
    let itemAt: (Int) -> QueueItem?
    /// Whether external (non-row) track drops are accepted into this lane. The
    /// manual lane inserts them; the context lane ignores them (you can't add a
    /// track into the release's own order) and only handles internal reorders.
    let acceptsExternalDrops: Bool
    @Binding
    var draggedEntryId: String?
    @Binding
    var dropInsertIndex: Int?
    /// Move the dragged entry to sit before `beforeEntryId`; `nil` = end.
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void

    /// The position of the dragged entry among this lane's *loaded* rows, or
    /// `nil` if it isn't dragging from this lane.
    private var fromIndex: Int? {
        guard let draggedId = draggedEntryId else {
            return nil
        }
        return (0..<count).first { itemAt($0)?.id == draggedId }
    }

    /// Whether this is an internal reorder within this lane (the dragged entry is
    /// one of this lane's rows). A drag from the other lane is neither an internal
    /// reorder here nor a valid external track drop, so it is rejected (cross-lane
    /// reorder is a core no-op anyway).
    private var isInternalDrag: Bool {
        fromIndex != nil
    }

    func dropEntered(info _: DropInfo) {
        if let fromIndex {
            // Internal reorder: show insertion line relative to source.
            if targetIndex > fromIndex {
                dropInsertIndex = targetIndex + 1
            }
            else if targetIndex < fromIndex {
                dropInsertIndex = targetIndex
            }
            else {
                dropInsertIndex = nil
            }
        }
        else if acceptsExternalDrops {
            // External drop into the manual lane: insertion line at the target.
            dropInsertIndex = targetIndex
        }
    }

    func dropUpdated(info _: DropInfo) -> DropProposal? {
        if isInternalDrag {
            return DropProposal(
                operation: targetLoaded ? .move : .forbidden
            )
        }
        return DropProposal(
            operation: acceptsExternalDrops ? .copy : .forbidden
        )
    }

    func performDrop(info: DropInfo) -> Bool {
        if let draggedId = draggedEntryId, let fromIndex, targetLoaded {
            // Internal reorder. `toIndex` is the gap the entry lands in; the
            // entry it lands before is whatever currently sits at that gap
            // (nil past the end = move to the lane's end).
            let toIndex =
                targetIndex > fromIndex ? targetIndex + 1 : targetIndex
            if toIndex != fromIndex {
                let beforeEntryId = toIndex < count ? itemAt(toIndex)?.id : nil
                onReorder(draggedId, beforeEntryId)
            }
            draggedEntryId = nil
            dropInsertIndex = nil
            return true
        }

        // External drop: rejected unless this lane accepts external tracks.
        guard acceptsExternalDrops else {
            dropInsertIndex = nil
            return false
        }

        // Load string IDs from the pasteboard.
        let providers = info.itemProviders(for: [UTType.plainText])
        guard !providers.isEmpty else {
            dropInsertIndex = nil
            return false
        }

        let insertAt = targetIndex
        let serialQueue = DispatchQueue(label: "bae.queue-drop-collect")
        nonisolated(unsafe) var collectedIds: [String] = []
        let group = DispatchGroup()

        for provider in providers {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) {
                item,
                _ in
                if let data = item as? Data,
                    let str = String(data: data, encoding: .utf8)
                {
                    serialQueue.sync { collectedIds.append(str) }
                }
                else if let str = item as? String {
                    serialQueue.sync { collectedIds.append(str) }
                }
                group.leave()
            }
        }

        group.notify(queue: .main) {
            // A dragged album card may carry several ids joined by a newline (a
            // multi-selection drag); split each back out before resolving.
            let ids = collectedIds.flatMap(AlbumDragPayload.decode)
            if !ids.isEmpty {
                onInsertTracks(ids, insertAt)
            }
        }

        dropInsertIndex = nil
        return true
    }

    func dropExited(info _: DropInfo) {
        dropInsertIndex = nil
    }

    func validateDrop(info: DropInfo) -> Bool {
        // Accept internal drags from this lane; accept external plain-text drops
        // only when this lane takes them.
        if isInternalDrag {
            return true
        }
        return acceptsExternalDrops
            && info.hasItemsConforming(to: [UTType.plainText])
    }
}

extension QueueView {
    /// Preview builder — fixes the image resolver and the action callbacks
    /// to inert defaults and injects `store` (queue state now flows through
    /// `@Environment`, not by-value props) so a preview states only its queue.
    @MainActor
    static func preview(
        isActive: Bool,
        nowPlayingTitle: String?,
        nowPlayingArtist: String?,
        store: PlaybackStore
    ) -> some View {
        QueueView(
            isActive: isActive,
            nowPlayingTitle: nowPlayingTitle,
            nowPlayingArtist: nowPlayingArtist,
            nowPlayingCover: nil,
            onClose: {},
            onClear: {},
            onSkipTo: { _ in },
            onRemove: { _ in },
            onReorder: { _, _ in },
            onInsertTracks: { _, _ in },
            onSetShuffle: { _ in }
        )
        .environment(store)
        .environment(Queue.stub)
    }
}

// MARK: - Previews

#Preview("With items") {
    QueueView.preview(
        isActive: true,
        nowPlayingTitle: PreviewData.nowPlayingTitle,
        nowPlayingArtist: PreviewData.nowPlayingArtist,
        store: PreviewData.queueStore(manualCount: 2, shuffled: true)
    )
    .frame(width: 350, height: 500)
    .environment(MediaPaths.stub)
}

#Preview("Empty") {
    QueueView.preview(
        isActive: false,
        nowPlayingTitle: nil,
        nowPlayingArtist: nil,
        store: PreviewData.queueStore(manualCount: 0, context: nil)
    )
    .frame(width: 350, height: 400)
    .environment(MediaPaths.stub)
}
