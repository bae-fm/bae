import SwiftUI
import UniformTypeIdentifiers

struct QueueView: View {
    let isActive: Bool
    let nowPlayingTitle: String?
    let nowPlayingArtist: String?
    let nowPlayingPath: String?
    /// The manual lane ("Up Next"): explicitly enqueued tracks, drained first.
    let manual: [QueueItem]
    /// The context (the release being played from), or `nil` when nothing plays
    /// from a release. Rendered as a section distinct from the manual lane.
    let context: QueuePlaybackContext?
    let resolveImagePath: (String?) -> String?
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

    private var isEmpty: Bool {
        // No context (nothing playing from a release) contributes no rows — a
        // normal state, named here rather than folded into a default.
        switch context {
        case .none:
            return manual.isEmpty
        case .some(let context):
            return manual.isEmpty && context.upcoming.isEmpty
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
                    guard !droppedIds.isEmpty else {
                        return false
                    }
                    onInsertTracks(droppedIds, 0)
                    return true
                }
            }
            else {
                ScrollView {
                    VStack(spacing: 0) {
                        // The manual lane drains first, so it is shown first. It
                        // accepts external track drops (Play Next / Add to Queue
                        // land here); the context section, being the release's own
                        // order, takes reorder/remove/skip only.
                        QueueSection(
                            title: String(localized: "Up Next"),
                            shuffled: false,
                            items: manual,
                            acceptsExternalDrops: true,
                            resolveImagePath: resolveImagePath,
                            draggedEntryId: $draggedEntryId,
                            onSkipTo: onSkipTo,
                            onRemove: onRemove,
                            onReorder: onReorder,
                            onInsertTracks: onInsertTracks,
                            onSetShuffle: nil,
                        )

                        if let context, !context.upcoming.isEmpty {
                            QueueSection(
                                title: String(localized: "Playing From"),
                                shuffled: context.shuffled,
                                items: context.upcoming,
                                acceptsExternalDrops: false,
                                resolveImagePath: resolveImagePath,
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
        ImageView(localPath: nowPlayingPath, pointSize: 48)
    }
}

// MARK: - Section

/// One lane of the queue (the manual "Up Next" lane or the context), rendered as
/// a labelled section of reorderable rows. Reorder/remove/skip operate by entry
/// id within this lane's `items`; only the manual lane sets `acceptsExternalDrops`
/// (external track drops land in the manual lane, never the release's own order).
/// Hover and drop-insertion are positional, so each section owns its own state.
private struct QueueSection: View {
    let title: String
    let shuffled: Bool
    let items: [QueueItem]
    let acceptsExternalDrops: Bool
    let resolveImagePath: (String?) -> String?
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

            ForEach(Array(items.enumerated()), id: \.element.id) {
                index,
                item in
                VStack(spacing: 0) {
                    // Kept in the tree and toggled by opacity so showing the drop
                    // line doesn't change a row's size and re-lay-out the lane.
                    insertionLine
                        .opacity(dropInsertIndex == index ? 1 : 0)
                        .allowsHitTesting(false)
                    queueItemRow(item, index: index)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 4)
                        .opacity(draggedEntryId == item.id ? 0.3 : 1.0)
                    Divider().padding(.leading, 62)
                }
                .onDrop(
                    of: [UTType.plainText],
                    delegate: QueueDropDelegate(
                        targetIndex: index,
                        items: items,
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
                .opacity(dropInsertIndex == items.count ? 1 : 0)
                .allowsHitTesting(false)

            // Trailing drop zone for appending — only the manual lane appends
            // external tracks; the context still accepts a reorder-to-end here.
            Color.clear
                .frame(height: acceptsExternalDrops ? 40 : 12)
                .onDrop(
                    of: [UTType.plainText],
                    delegate: QueueDropDelegate(
                        targetIndex: items.count,
                        items: items,
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
            queueItemArt(path: resolveImagePath(item.coverImageId))
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

    private func queueItemArt(path: String?) -> some View {
        ImageView(localPath: path, pointSize: 40)
    }
}

// MARK: - Drop Delegate

private struct QueueDropDelegate: DropDelegate {
    let targetIndex: Int
    let items: [QueueItem]
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

    /// Whether this is an internal reorder within this lane (the dragged entry is
    /// one of this lane's rows). A drag from the other lane is neither an internal
    /// reorder here nor a valid external track drop, so it is rejected (cross-lane
    /// reorder is a core no-op anyway).
    private var isInternalDrag: Bool {
        guard let draggedId = draggedEntryId else {
            return false
        }
        return items.contains { $0.id == draggedId }
    }

    func dropEntered(info _: DropInfo) {
        if let draggedId = draggedEntryId,
            let fromIndex = items.firstIndex(where: { $0.id == draggedId })
        {
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
            return DropProposal(operation: .move)
        }
        return DropProposal(
            operation: acceptsExternalDrops ? .copy : .forbidden
        )
    }

    func performDrop(info: DropInfo) -> Bool {
        if let draggedId = draggedEntryId,
            let fromIndex = items.firstIndex(where: { $0.id == draggedId })
        {
            // Internal reorder. `toIndex` is the gap the entry lands in; the
            // entry it lands before is whatever currently sits at that gap
            // (nil past the end = move to the lane's end).
            let toIndex =
                targetIndex > fromIndex ? targetIndex + 1 : targetIndex
            if toIndex != fromIndex {
                let beforeEntryId =
                    toIndex < items.count ? items[toIndex].id : nil
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
            if !collectedIds.isEmpty {
                onInsertTracks(collectedIds, insertAt)
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
    /// to inert defaults so a preview states only its queue state.
    @MainActor
    static func preview(
        isActive: Bool,
        nowPlayingTitle: String?,
        nowPlayingArtist: String?,
        manual: [QueueItem],
        context: QueuePlaybackContext?
    ) -> QueueView {
        QueueView(
            isActive: isActive,
            nowPlayingTitle: nowPlayingTitle,
            nowPlayingArtist: nowPlayingArtist,
            nowPlayingPath: nil,
            manual: manual,
            context: context,
            resolveImagePath: { _ in nil },
            onClose: {},
            onClear: {},
            onSkipTo: { _ in },
            onRemove: { _ in },
            onReorder: { _, _ in },
            onInsertTracks: { _, _ in },
            onSetShuffle: { _ in }
        )
    }
}

// MARK: - Previews

#Preview("With items") {
    QueueView.preview(
        isActive: true,
        nowPlayingTitle: PreviewData.nowPlayingTitle,
        nowPlayingArtist: PreviewData.nowPlayingArtist,
        manual: Array(PreviewData.queueItems.prefix(2)),
        context: QueuePlaybackContext(
            shuffled: true,
            upcoming: Array(PreviewData.queueItems.suffix(3))
        )
    )
    .frame(width: 350, height: 500)
    .environment(MediaPaths.stub)
}

#Preview("Empty") {
    QueueView.preview(
        isActive: false,
        nowPlayingTitle: nil,
        nowPlayingArtist: nil,
        manual: [],
        context: nil
    )
    .frame(width: 350, height: 400)
    .environment(MediaPaths.stub)
}
