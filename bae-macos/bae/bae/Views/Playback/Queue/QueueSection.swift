import BaeKit
import SwiftUI
import UniformTypeIdentifiers

/// How many rows to load around an unloaded context row.
private let queueUpcomingLoadBatchSize = 100

// periphery:ignore
/// A row's load task identity: the queue revision, so a new revision restarts
/// the load, and the absolute index.
private struct QueueRowLoadID: Hashable {
    let epoch: UInt64
    let index: Int
}

/// A row's `ForEach` identity: the entry id when loaded, so SwiftUI animates a
/// reorder, else the display slot. `sourceIndex` is the canonical index
/// `itemAt`/`loadRange` take; it differs from `displaySlot` only during a drag.
private struct QueueRowSlot: Identifiable {
    let id: String
    let displaySlot: Int
    let sourceIndex: Int
}

/// One queue lane (the manual "Up Next" lane or the context) as a labelled
/// section of reorderable rows. `itemAt` returns `nil` for an unloaded context
/// row, which shows a placeholder and calls `loadRange`.
struct QueueSection: View {
    /// A row's inset from the lane's edges, and the gap either side of it.
    private static let rowInset = ThemeSpace.related
    private static let rowGap = ThemeSpace.hairline
    /// Every row's height, declared rather than measured because the lazy stack
    /// only estimates unbuilt rows and `QueueDragCoordinator` divides by it.
    static let rowHeight =
        QueueItemRow.artworkSize + 2 * (QueueItemRow.verticalInset + rowGap)

    /// `nil` hides the header label, for when only one list is visible.
    let title: String?
    let shuffled: Bool
    let count: Int
    let itemAt: (Int) -> QueueItem?
    /// Restarts in-flight row loads when it changes; fixed at 0 for the manual
    /// lane, which is always loaded.
    let loadEpoch: UInt64
    /// Fetch `[offset, offset + limit)` and merge it into the store. `nil` for
    /// the manual lane, which is never windowed.
    let loadRange: ((_ offset: Int, _ limit: Int) async -> Void)?
    let acceptsExternalDrops: Bool
    /// This lane's key in the drag coordinator.
    let laneId: QueueLaneID
    /// The shared drag state every row shift is derived from.
    let coordinator: QueueDragCoordinator
    /// The store's queue revision; a change drops the held post-commit order.
    let queueRevision: UInt64
    /// Empties this lane; offered only while the lane has rows.
    let onClear: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Sets this lane's shuffle; `nil` on the manual lane, which has none.
    let onSetShuffle: ((Bool) -> Void)?

    /// The hovered row by entry id, not slot, so the hover stays on its row
    /// when a removal above shifts the lane without new hover events.
    @State
    private var hoveredEntryId: String?
    @State
    private var dropInsertIndex: Int?
    /// Rows collapsed before core confirms the removal, so the click responds
    /// at once; cleared when the entry leaves the lane.
    @State
    private var removingEntryIds: Set<String> = []

    /// Display slots 0..<count resolved to source index and identity, mapped
    /// lazily so `itemAt` runs only for the rows the lazy stack builds.
    private var rowSlots: LazyMapCollection<Range<Int>, QueueRowSlot> {
        // Derived from the current count so a queue change mid-drag can't
        // leave a stale permutation.
        let order = coordinator.displayOrder(for: laneId, count: count)
        return (0..<count).lazy
            .map { displaySlot in
                let sourceIndex = order?[displaySlot] ?? displaySlot
                let id = itemAt(sourceIndex)?.id ?? "unloaded-\(displaySlot)"
                return QueueRowSlot(
                    id: id,
                    displaySlot: displaySlot,
                    sourceIndex: sourceIndex
                )
            }
    }

    var body: some View {
        VStack(spacing: 0) {
            sectionHeader

            // Lazy because the context lane can span the whole library; rows
            // set their own gap.
            LazyVStack(spacing: 0) {
                ForEach(rowSlots) { slot in
                    let item = itemAt(slot.sourceIndex)
                    let isFloating =
                        item != nil
                        && coordinator.floatingEntryId(in: laneId) == item?.id
                    let isRemoving =
                        item.map { removingEntryIds.contains($0.id) } ?? false
                    queueRow(item, index: slot.displaySlot)
                        .padding(.horizontal, Self.rowInset)
                        .padding(.vertical, Self.rowGap)
                        // Loaded or placeholder, every row takes the pitch the
                        // coordinator's slot math assumes.
                        .frame(height: Self.rowHeight)
                        // Removal clips the row from the bottom; the collapse
                        // rides the remove action's spring, the fade its own
                        // curve.
                        .frame(height: isRemoving ? 0 : nil, alignment: .top)
                        .clipped()
                        .opacity(isRemoving ? 0 : 1)
                        .animation(.linear(duration: 0.25), value: isRemoving)
                        // The manual lane's insertion line sits on the bottom
                        // of the row above the gap; gap 0 uses row 0's top.
                        .overlay(alignment: .bottom) {
                            if acceptsExternalDrops,
                                insertGapForLine == slot.displaySlot + 1
                            {
                                insertionLine
                                    .allowsHitTesting(false)
                            }
                        }
                        .overlay(alignment: .top) {
                            if acceptsExternalDrops, slot.displaySlot == 0,
                                insertGapForLine == 0
                            {
                                insertionLine
                                    .allowsHitTesting(false)
                            }
                        }
                        // The dragged row keeps its slot and gesture but is
                        // hidden while the floating copy draws it.
                        .opacity(isFloating ? 0 : 1)
                        .task(
                            id: QueueRowLoadID(
                                epoch: loadEpoch,
                                index: slot.sourceIndex
                            )
                        ) {
                            guard item == nil, let loadRange else {
                                return
                            }
                            let first = max(
                                0,
                                slot.sourceIndex - queueUpcomingLoadBatchSize
                                    / 2
                            )
                            let end = min(
                                first + queueUpcomingLoadBatchSize,
                                count
                            )
                            await loadRange(first, end - first)
                        }
                        .onDrop(
                            of: [UTType.plainText],
                            delegate: QueueDropDelegate(
                                targetIndex: slot.displaySlot,
                                acceptsExternalDrops: acceptsExternalDrops,
                                dropInsertIndex: $dropInsertIndex,
                                onInsertTracks: onInsertTracks,
                            )
                        )
                }
            }
            .overlay(alignment: .top) { draggedRowCopy }
            // The coordinator maps cursor positions onto slots with this frame.
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .named("queuePane"))
            } action: { frame in
                coordinator.setRowsGeometry(
                    laneId,
                    frame: frame,
                    rowCount: count
                )
            }
            // Only an empty lane needs this line; otherwise the last row's
            // bottom overlay marks the append gap.
            if acceptsExternalDrops, count == 0 {
                insertionLine
                    .opacity(insertGapForLine == 0 ? 1 : 0)
                    .allowsHitTesting(false)
            }

            // The manual lane's append drop zone and cross-lane append target;
            // a thin spacer on the context lane.
            Color.clear
                .frame(
                    height: acceptsExternalDrops
                        ? ThemeSpace.edge : ThemeSpace.related
                )
                .onGeometryChange(for: CGRect.self) { proxy in
                    proxy.frame(in: .named("queuePane"))
                } action: { frame in
                    coordinator.setAppendFrame(laneId, frame: frame)
                }
                .onDrop(
                    of: [UTType.plainText],
                    delegate: QueueDropDelegate(
                        targetIndex: count,
                        acceptsExternalDrops: acceptsExternalDrops,
                        dropInsertIndex: $dropInsertIndex,
                        onInsertTracks: onInsertTracks,
                    )
                )
        }
        // Core applied the change, so the canonical order now matches the held
        // one and dropping it doesn't snap.
        .onChange(of: queueRevision) {
            coordinator.clearHold(laneId)
            // Drop only ids whose rows are gone: the snapshot and revision
            // update separately, and a still-present row would jump back open.
            removingEntryIds = removingEntryIds.filter { id in
                (0..<count).contains { itemAt($0)?.id == id }
            }
        }
        // Drop the hover when a drag starts, since `.onHover` won't fire again
        // until it ends.
        .onChange(of: coordinator.isDragging) {
            if coordinator.isDragging {
                hoveredEntryId = nil
            }
        }
    }
}

/// `QueueSection`'s drag resolution, header, and row builders.
extension QueueSection {
    /// The manual lane's insertion gap from an external drop or a cross-lane
    /// drag; `nil` hides the line.
    private var insertGapForLine: Int? {
        guard acceptsExternalDrops else {
            return nil
        }
        return dropInsertIndex
            ?? coordinator.manualInsertGap(manualCount: count)
    }

    /// Resolves a finished drag: a reorder commits unless the row after the
    /// drop slot isn't loaded yet, a context row dropped on the manual lane
    /// enqueues its track, and anything else settles back.
    private func handleDragEnd() {
        // A cross-lane drop skips the animation so the eye stays on the
        // enqueued row, not the source row flying home.
        guard !coordinator.isCrossLaneTargeting else {
            finishDrag()
            coordinator.settled()
            return
        }
        // The floating copy glides into its slot under this spring.
        withAnimation(.snappy(duration: 0.2, extraBounce: 0)) {
            finishDrag()
        } completion: {
            coordinator.settled()
        }
    }

    private func finishDrag() {
        let outcome = coordinator.finish(count: count)
        switch outcome {
        case .none:
            break
        case .insertIntoManual(let trackId, let gap):
            onInsertTracks([trackId], gap)
        case .reorder(let entryId, let finalOrder, let gap, let lane):
            if gap + 1 < finalOrder.count {
                guard let beforeEntryId = itemAt(finalOrder[gap + 1])?.id
                else {
                    return
                }
                coordinator.holdOrder(finalOrder, lane: lane)
                onReorder(entryId, beforeEntryId)
                return
            }
            coordinator.holdOrder(finalOrder, lane: lane)
            onReorder(entryId, nil)
        }
    }

    @ViewBuilder
    private var sectionHeader: some View {
        if title != nil || onSetShuffle != nil || count > 0 {
            HStack(spacing: ThemeSpace.compact) {
                if let title {
                    Eyebrow(verbatim: title)
                        .lineLimit(1)
                }
                Spacer()
                if count > 0 {
                    clearButton
                }
                if let onSetShuffle {
                    shuffleToggle(onSetShuffle)
                }
            }
            // In line with the rows' content, so Clear and shuffle sit over the
            // remove buttons.
            .padding(
                .horizontal,
                Self.rowInset + QueueItemRow.horizontalInset
            )
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.inline)
        }
    }

    /// This lane's Clear; the tooltip and accessibility label name the lane.
    private var clearButton: some View {
        Button(action: onClear) {
            Eyebrow("Clear")
        }
        .buttonStyle(.plain)
        .help(clearLaneLabel)
        .accessibilityLabel(clearLaneLabel)
    }

    /// What this lane's Clear empties, named for the tooltip and VoiceOver.
    private var clearLaneLabel: String {
        switch laneId {
        case .manual:
            return String(localized: "Clear Up Next")
        case .context:
            return String(localized: "Clear Playing From")
        }
    }

    /// The context's shuffle toggle; the current track keeps playing.
    private func shuffleToggle(_ onSetShuffle: @escaping (Bool) -> Void)
        -> some View
    {
        Button {
            onSetShuffle(!shuffled)
        } label: {
            Image(systemName: "shuffle")
                .themeIcon(.small)
                .foregroundStyle(shuffled ? Theme.accent : .secondary)
                // The same slot as the rows' remove button, so they line up.
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .fill(shuffled ? Theme.accentSoft : .clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(PressableIconButtonStyle())
        .help(shuffled ? "Turn off shuffle" : "Shuffle")
        .accessibilityLabel(shuffled ? "Turn off shuffle" : "Shuffle")
    }

    private var insertionLine: some View {
        Rectangle()
            .fill(Color.accentColor)
            .frame(height: 2)
            .padding(.horizontal, ThemeSpace.related)
    }

    /// The dragged row drawn at the cursor, then gliding into its slot. A copy,
    /// because `zIndex` has no effect in a lazy stack and a row raised in place
    /// would draw under the rows below it.
    @ViewBuilder
    private var draggedRowCopy: some View {
        if let floating = coordinator.floatingRow(in: laneId) {
            let isDragging = coordinator.isDragging
            // The row as a value, not an index lookup: after core echoes the
            // reorder, the old index belongs to another row.
            QueueItemRow(
                item: floating.item,
                isHovered: false,
                onHoverChanged: { _ in },
                onSkipTo: { _ in },
                onRemove: { _ in },
            )
            .padding(.horizontal, Self.rowInset)
            .padding(.vertical, Self.rowGap)
            .frame(height: Self.rowHeight)
            .shadow(
                color: isDragging ? Theme.shadow : Color.clear,
                radius: 6,
                y: 2
            )
            .allowsHitTesting(false)
            .offset(y: floating.top)
            // No animation while dragging so the copy stays under the cursor;
            // the release rides the siblings' spring.
            .transaction { transaction in
                if isDragging {
                    transaction.animation = nil
                }
            }
        }
    }

    /// A row at `index`: the resolved item, or a placeholder while it loads.
    private func queueRow(_ item: QueueItem?, index: Int) -> some View {
        Group {
            if let item {
                QueueItemRow(
                    item: item,
                    // No hover during a drag: rows sliding under the pointer
                    // fire hover events.
                    isHovered: hoveredEntryId == item.id
                        && !coordinator.isDragging,
                    onHoverChanged: { hovering in
                        guard !coordinator.isDragging else {
                            return
                        }
                        if hovering {
                            hoveredEntryId = item.id
                        }
                        else if hoveredEntryId == item.id {
                            // Only the owning row clears the hover: a
                            // collapsing row's exit can arrive after the next
                            // row claimed it.
                            hoveredEntryId = nil
                        }
                    },
                    onSkipTo: onSkipTo,
                    onRemove: { id in
                        // Collapse first and send the remove after the spring
                        // settles, so core's echo can't cut the animation
                        // short.
                        withAnimation(.spring(duration: 0.2)) {
                            _ = removingEntryIds.insert(id)
                        }
                        Task { @MainActor in
                            try? await Task.sleep(for: .milliseconds(350))
                            onRemove(id)
                        }
                    }
                )
                // An in-process reorder drag rather than an AppKit drag
                // session, so `onEnded` reliably ends it; `minimumDistance`
                // keeps clicks working.
                .gesture(
                    DragGesture(
                        minimumDistance: 4,
                        coordinateSpace: .named("queuePane")
                    )
                    .onChanged { value in
                        if coordinator.isDragging {
                            withAnimation(
                                .snappy(duration: 0.15, extraBounce: 0)
                            ) {
                                coordinator.update(location: value.location)
                            }
                        }
                        else {
                            // A drag begins only in canonical order, so the
                            // display slot is the canonical slot.
                            coordinator.begin(
                                lane: laneId,
                                item: item,
                                startSlot: index,
                                location: value.location
                            )
                        }
                    }
                    .onEnded { _ in
                        handleDragEnd()
                    }
                )
            }
            else {
                QueuePlaceholderRow()
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Hosts one lane over a fresh drag coordinator and the `queuePane`
    /// coordinate space, with rows from the shared queue fixtures.
    @MainActor
    private func queueSectionPreview(
        title: String?,
        shuffled: Bool,
        count: Int,
        acceptsExternalDrops: Bool,
        laneId: QueueLaneID,
        onSetShuffle: ((Bool) -> Void)? = nil
    ) -> some View {
        let items = Array(PreviewData.queueItems.prefix(count))
        return QueueSection(
            title: title,
            shuffled: shuffled,
            count: count,
            itemAt: { index in
                items.indices.contains(index) ? items[index] : nil
            },
            loadEpoch: 0,
            loadRange: nil,
            acceptsExternalDrops: acceptsExternalDrops,
            laneId: laneId,
            coordinator: QueueDragCoordinator(),
            queueRevision: 1,
            onClear: {},
            onSkipTo: { _ in },
            onRemove: { _ in },
            onReorder: { _, _ in },
            onInsertTracks: { _, _ in },
            onSetShuffle: onSetShuffle,
        )
        .frame(width: 400)
        .padding(.vertical)
        .background(Theme.surface)
        .coordinateSpace(name: "queuePane")
        .environment(ImageStore.stub())
    }

    #Preview("Up Next lane") {
        queueSectionPreview(
            title: "Up Next",
            shuffled: false,
            count: 2,
            acceptsExternalDrops: true,
            laneId: .manual,
            onSetShuffle: nil,
        )
    }

    #Preview("Context lane — shuffled") {
        queueSectionPreview(
            title: "Playing From · Neon Frequencies",
            shuffled: true,
            count: 5,
            acceptsExternalDrops: false,
            laneId: .context,
            onSetShuffle: { _ in },
        )
    }
#endif

/// Which queue lane a section renders.
enum QueueLaneID {
    case manual
    case context
}
