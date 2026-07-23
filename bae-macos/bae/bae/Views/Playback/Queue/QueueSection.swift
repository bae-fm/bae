import BaeKit
import SwiftUI
import UniformTypeIdentifiers

/// Range fetched around an unloaded context row as the queue scrolls, mirroring
/// the page size `PlaybackStore.loadUpcomingRange`'s bridge call uses.
private let queueUpcomingLoadBatchSize = 100

// periphery:ignore
/// Row load identity: which section epoch (the store's queue revision, so a
/// bump restarts every visible row's load task) and which absolute index. The
/// manual lane, always fully loaded, passes a constant epoch and a `nil` load
/// hook so its rows never fire a load.
private struct QueueRowLoadID: Hashable {
    let epoch: UInt64
    let index: Int
}

/// A row's identity in the `ForEach` below: a loaded row is keyed by its entry
/// id (stable across a live reorder, which is what lets SwiftUI animate the
/// move); an unloaded row is keyed by its display slot instead, since it has no
/// content of its own to identify. `sourceIndex` is the absolute, canonical
/// index `itemAt`/`loadRange` address — it equals `displaySlot` outside a live
/// reorder, and diverges from it only while the coordinator's display order
/// permutes the lane (`QueueDragCoordinator.displayOrder`).
private struct QueueRowSlot: Identifiable {
    let id: String
    let displaySlot: Int
    let sourceIndex: Int
}

/// One lane of the queue (the manual "Up Next" lane or the context), rendered as
/// a labelled section of reorderable rows addressed by absolute index —
/// `itemAt(index)` resolves the loaded row, or `nil` for a not-yet-loaded
/// context row, which renders a placeholder and triggers `loadRange`. Only the
/// manual lane sets `acceptsExternalDrops` (external track drops land in the
/// manual lane, never the release's own order). Hover and drop-insertion are
/// positional, so each section owns its own state.
struct QueueSection: View {
    /// `nil` hides the header label: with an empty manual lane there is only
    /// one visible list, and "Up Next" / "Playing From" labels over a single
    /// list are noise. The shuffle control keeps its slot regardless.
    let title: String?
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
    /// Which lane this section renders — the drag coordinator's key for
    /// geometry, permutations, and cross-lane targeting.
    let laneId: QueueLaneID
    /// The shared drag state: the active gesture, lane geometry, and the
    /// post-commit hold. All row-shift effects derive from it per render.
    let coordinator: QueueDragCoordinator
    /// The store's queue revision, independent of `loadEpoch` (which the manual
    /// lane fixes at 0). A change means core applied a mutation — including the
    /// reorder this lane just committed — so the held post-commit order is
    /// dropped: the canonical order the new snapshot carries already equals
    /// what was on screen, so clearing it is visually a no-op, not a snap.
    let queueRevision: UInt64
    /// Empty this lane. Shown in the section header only while the lane has
    /// rows — an empty section has nothing to clear, so it offers nothing.
    let onClear: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Flip this section between sequential and shuffled order, given its current
    /// `shuffled` state. `nil` on the manual lane, which has no shuffle control.
    let onSetShuffle: ((Bool) -> Void)?

    /// The hovered row, by entry id — NOT by display slot: a removal above the
    /// pointer slides the lane up and re-indexes every row without firing new
    /// hover events, so a stored slot ends up pointing one row past the one
    /// actually under the pointer. The id follows the row wherever it lands.
    @State
    private var hoveredEntryId: String?
    @State
    private var dropInsertIndex: Int?
    /// Rows removed optimistically: the X collapses the row on the spot, the
    /// remove command races the snapshot behind the animation, and the
    /// revision bump (whose canonical order no longer carries the entry)
    /// clears the set. The alternative — waiting for the round trip — reads
    /// as the click not registering.
    @State
    private var removingEntryIds: Set<String> = []

    /// Display slots 0..<count, each resolved to its source index (identity
    /// in canonical order, or permuted while a drag is live) and an identity —
    /// the entry id when loaded, a slot sentinel when not. A `LazyMapCollection`
    /// rather than a materialized array, so no `[QueueRowSlot]` is allocated
    /// per render; `itemAt` still runs for every index the `ForEach` renders —
    /// which, in the sections' plain `VStack`, is all of them.
    private var rowSlots: LazyMapCollection<Range<Int>, QueueRowSlot> {
        // Derived fresh against the CURRENT count on every render — a queue
        // change mid-drag can't leave a stale-length permutation.
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

            // An explicit zero-spacing stack, NOT `Group`: a modified Group
            // wraps its children in an implicit container with DEFAULT stack
            // spacing — which put a phantom 8pt between every row (53pt pitch
            // for 45pt rows), held collapse animations 8pt short until the
            // unmount, and floated the perceived row border 8pt above each
            // row's actual top edge.
            VStack(spacing: 0) {
                ForEach(rowSlots) { slot in
                    let item = itemAt(slot.sourceIndex)
                    let isDragged =
                        item != nil
                        && coordinator.draggedEntryId(in: laneId) == item?.id
                    let isRemoving =
                        item.map { removingEntryIds.contains($0.id) } ?? false
                    queueRow(item, index: slot.displaySlot)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 1)
                        // Optimistic removal: one linear curve drives the fade
                        // and the collapse together. The content stays full-size,
                        // pinned to the top — the bottom edge rises over it (a
                        // top-aligned zero frame + clip), never squishing it. The
                        // clip sits BEFORE the drag offset in the chain, so it
                        // clips in the row's own space and cannot clip a dragged
                        // row's translated rendering.
                        // No scoped animation on the frame: the collapse rides
                        // the withAnimation transaction from the remove action, so
                        // every downstream layout shift (rows below, the whole
                        // next section) animates in the SAME spring — a scoped
                        // animation here moved only this row smoothly and let the
                        // rest snap. The fade alone keeps its own linear curve.
                        .frame(height: isRemoving ? 0 : nil, alignment: .top)
                        .clipped()
                        .opacity(isRemoving ? 0 : 1)
                        .animation(.linear(duration: 0.25), value: isRemoving)
                        // The manual lane's insertion line serves both external
                        // track drops and a context-row drag hovering here (a
                        // cross-lane enqueue); the context lane never shows one.
                        // Anchored to the BOTTOM of the row above the gap; a
                        // top-anchored line on the row below kept landing visibly
                        // under it. Gap 0 (above the first row) is the one gap
                        // with no row above; it anchors to row 0's top instead.
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
                        // The dragged row tracks the cursor exactly (offset from
                        // whatever display slot it currently occupies), floats over
                        // its siblings — across the lane boundary too — and never
                        // animates; the siblings do, as they shift around it.
                        .offset(
                            y: isDragged
                                ? coordinator.draggedRowOffset(
                                    displaySlot: slot.displaySlot
                                ) : 0
                        )
                        .zIndex(isDragged ? 1 : 0)
                        .shadow(
                            color: .black.opacity(isDragged ? 0.25 : 0),
                            radius: 6,
                            y: 2
                        )
                        .transaction { transaction in
                            if isDragged {
                                transaction.animation = nil
                            }
                        }
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
            // The rows region's frame in the pane space: the coordinator maps
            // cursor positions onto row slots with it (height / count = the
            // uniform row height).
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .named("queuePane"))
            } action: { frame in
                coordinator.setRowsGeometry(
                    laneId,
                    frame: frame,
                    rowCount: count
                )
            }
            // The trailing drop line: only the EMPTY lane needs it — with rows
            // present, the last row's bottom overlay above marks the append
            // gap.
            if acceptsExternalDrops, count == 0 {
                insertionLine
                    .opacity(insertGapForLine == 0 ? 1 : 0)
                    .allowsHitTesting(false)
            }

            // Trailing drop zone for appending external tracks — manual lane
            // only; the context lane keeps a thin spacer between sections. Its
            // frame doubles as the cross-lane append target — the ONLY target
            // when the lane has no rows yet. Kept slim so the two sections sit
            // close; the append gap is still hittable via the last row's lower
            // half.
            Color.clear
                .frame(height: acceptsExternalDrops ? 18 : 8)
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
        // The reorder this lane committed just echoed back from core: the
        // canonical order now equals the held one, so dropping the hold is a
        // visual no-op, not a snap.
        .onChange(of: queueRevision) {
            coordinator.clearHold(laneId)
            // Drop only ids whose rows are GONE from the lane data. The
            // snapshot and the revision arrive as separate observable
            // updates, so clearing unconditionally can hit a pass where the
            // row still exists — its zero frame snaps back to intrinsic for
            // a frame (a visible end-of-collapse jump).
            removingEntryIds = removingEntryIds.filter { id in
                (0..<count).contains { itemAt($0)?.id == id }
            }
        }
        // A drag started: drop the stored hover. `.onHover` won't fire again
        // until the gesture ends, so without this the pre-drag value would
        // resurface afterwards.
        .onChange(of: coordinator.isDragging) {
            if coordinator.isDragging {
                hoveredEntryId = nil
            }
        }
    }
}

/// `QueueSection`'s drop-resolution, header chrome, and per-row builders, split
/// into an extension purely to keep the primary type body under the length
/// limit — no behavior or state change; each method still runs on the section
/// instance and reads its stored props and `@State` directly.
extension QueueSection {
    /// Where the manual lane's insertion line sits: an external album-card
    /// drag's drop target, or a context-row drag hovering here (cross-lane
    /// enqueue). `nil` hides it.
    private var insertGapForLine: Int? {
        guard acceptsExternalDrops else {
            return nil
        }
        return dropInsertIndex
            ?? coordinator.manualInsertGap(manualCount: count)
    }

    /// Resolve a finished drag in this lane: a same-lane reorder commits
    /// `(entryId, beforeEntryId)` (refused when the anchor row after the drop
    /// slot exists but isn't loaded — core pins by id, and there is none yet);
    /// a context-row release over the manual lane enqueues the track there;
    /// anything else settles back.
    private func handleDragEnd() {
        // A cross-lane commit ends WITHOUT animation: the enqueued row appears
        // at the drop point when the snapshot lands (milliseconds), and
        // animating the source row's flight back home drags the eye away from
        // it. Same-lane outcomes (commit or settle-back) animate the release.
        let animation: Animation? =
            coordinator.isCrossLaneTargeting
            ? nil : .snappy(duration: 0.2, extraBounce: 0)
        withAnimation(animation) {
            finishDrag()
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
            HStack(spacing: 6) {
                if let title {
                    Text(title)
                        .font(.system(size: 10, weight: .bold))
                        .tracking(1.2)
                        .textCase(.uppercase)
                        .foregroundStyle(.secondary)
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
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .padding(.bottom, 4)
        }
    }

    /// This lane's Clear. The visible word stays "Clear" — the header beside it
    /// already names the lane — while the tooltip and the accessibility label
    /// spell out which lane it empties, for anyone reading it out of context.
    private var clearButton: some View {
        Button("Clear") { onClear() }
            .buttonStyle(.plain)
            .font(.system(size: 10, weight: .bold))
            .tracking(1.2)
            .textCase(.uppercase)
            .foregroundStyle(.secondary)
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

    /// The context's shuffle toggle: tinted when on, muted when off; tapping it
    /// flips the context's order while the current track keeps playing.
    private func shuffleToggle(_ onSetShuffle: @escaping (Bool) -> Void)
        -> some View
    {
        Button {
            onSetShuffle(!shuffled)
        } label: {
            Image(systemName: "shuffle")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(shuffled ? Theme.accent : .secondary)
                // Same 28pt slot as the rows' X column, so the glyphs align
                // vertically — and the same comfortable hit target.
                .frame(width: 28, height: 28)
                .background(
                    RoundedRectangle(cornerRadius: 7)
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
            .padding(.horizontal, 8)
    }

    /// A row at `index`: the resolved item, or a placeholder while it loads.
    private func queueRow(_ item: QueueItem?, index: Int) -> some View {
        Group {
            if let item {
                QueueItemRow(
                    item: item,
                    // Hover chrome is suppressed for the whole drag: rows
                    // sliding under the stationary pointer fire hover events
                    // mid-drag — without the guard, whichever row slides by
                    // wears the remove/play chrome.
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
                            // Only the row that owns the hover clears it: a
                            // collapsing row's exit event can land AFTER the
                            // row sliding into its place claimed the hover,
                            // and must not wipe that claim.
                            hoveredEntryId = nil
                        }
                    },
                    onSkipTo: onSkipTo,
                    onRemove: { id in
                        // The exit animation owns the removal visually; the
                        // command follows as it finishes (sent immediately,
                        // core's echo lands in ~20ms, unmounts the row, and
                        // truncates the exit to an imperceptible blink). The
                        // global transaction is what carries the spring to
                        // every row and section this collapse displaces.
                        // Spring perceptual duration 0.2 settles well before
                        // the 350ms command — an echo landing inside the
                        // spring's tail truncates it into a visible end nudge.
                        withAnimation(.spring(duration: 0.2)) {
                            _ = removingEntryIds.insert(id)
                        }
                        Task { @MainActor in
                            try? await Task.sleep(for: .milliseconds(350))
                            onRemove(id)
                        }
                    }
                )
                // The reorder drag, in-process: the row follows the cursor,
                // siblings shift continuously at slot boundaries, and
                // `onEnded` is the one deterministic end-of-drag signal —
                // no AppKit drag session, no floating drag image to linger
                // after release. `minimumDistance` keeps clicks (skip,
                // hover buttons, double-click) intact.
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
                            // `index` is this row's display slot; a drag can
                            // only begin from canonical order (no permutation
                            // is live), so it is also the canonical slot.
                            coordinator.begin(
                                lane: laneId,
                                entryId: item.id,
                                trackId: item.trackId,
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
