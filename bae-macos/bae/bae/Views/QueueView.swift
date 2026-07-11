import AppKit
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

    // A drag can start in either section and target the other (a context row
    // enqueues into the manual lane), so the coordinator is shared; hover and
    // external-drop insertion stay per-section (positional — a row index in
    // one lane must not light up the same index in the other).
    @State
    private var dragCoordinator = QueueDragCoordinator()

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
                // No overlay scroller: inside the fixed-size popover it only
                // ever showed up as a flash when the pane animates in.
                ScrollView {
                    LazyVStack(spacing: 0) {
                        // The manual lane drains first, so it is shown first. It
                        // accepts external track drops (Play Next / Add to Queue
                        // land here); the context section, being the release's own
                        // order, takes reorder/remove/skip only. It is always
                        // fully resolved, so it never has a load hook or unloaded
                        // rows.
                        QueueSection(
                            title: manual.isEmpty
                                ? nil : String(localized: "Up Next"),
                            shuffled: false,
                            count: manual.count,
                            itemAt: { index in
                                manual.indices.contains(index)
                                    ? manual[index] : nil
                            },
                            loadEpoch: 0,
                            loadRange: nil,
                            acceptsExternalDrops: true,
                            laneId: .manual,
                            coordinator: dragCoordinator,
                            queueRevision: playbackStore.revision,
                            onSkipTo: onSkipTo,
                            onRemove: onRemove,
                            onReorder: onReorder,
                            onInsertTracks: onInsertTracks,
                            onSetShuffle: nil,
                        )
                        .zIndex(dragCoordinator.isDragSource(.manual) ? 1 : 0)

                        if let context, context.upcomingTotal > 0 {
                            // The context tail is library-scaled and only
                            // partly resolved (`upcomingItem` returns `nil` for
                            // an index not yet loaded); the section renders a
                            // placeholder for those rows and fetches the range
                            // around them.
                            QueueSection(
                                title: manual.isEmpty
                                    ? nil
                                    : Self.contextSectionTitle(context.kind),
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
                                laneId: .context,
                                coordinator: dragCoordinator,
                                queueRevision: playbackStore.revision,
                                onSkipTo: onSkipTo,
                                onRemove: onRemove,
                                onReorder: onReorder,
                                onInsertTracks: onInsertTracks,
                                onSetShuffle: onSetShuffle,
                            )
                            .zIndex(
                                dragCoordinator.isDragSource(.context) ? 1 : 0
                            )
                        }
                    }
                }
                .coordinateSpace(name: "queuePane")
                .scrollIndicators(.never)
                .onChange(of: manual.count, initial: true) {
                    // Cross-lane gap math runs in the context section's
                    // gesture, which can't see the manual section's props.
                    dragCoordinator.manualGapCount = manual.count
                }
            }
        }
        // No background of its own: QueuePanel supplies the panel material —
        // an opaque fill here would block it.
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
        }
        .padding()
    }

    // MARK: - Now Playing

    private var nowPlayingSection: some View {
        HStack(spacing: 10) {
            nowPlayingArt
                .frame(width: 40, height: 40)
                .clipShape(RoundedRectangle(cornerRadius: 3))

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
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    private var nowPlayingArt: some View {
        ImageView(content: nowPlayingCover, pointSize: 40)
    }
}

// MARK: - Drag coordinator

/// Which queue lane a section renders.
enum QueueLaneID {
    case manual
    case context
}

/// A lane's geometry in the pane coordinate space, captured off the live
/// layout. `rowHeight` is the rows-region height over the row count — rows
/// are uniform by design (hover chrome toggles by opacity precisely so they
/// never resize); zero while the lane is empty. `appendFrame` is the lane's
/// trailing zone: the region past the last row that targets an
/// insert-at-end — the whole target when the lane has no rows at all.
private struct QueueLaneGeometry {
    var rowsFrame: CGRect = .null
    var rowHeight: CGFloat = 0
    var appendFrame: CGRect = .null
}

/// The in-flight reorder drag. Owned by the row's own `DragGesture` — no
/// AppKit drag session, no floating drag image, no item providers: the row
/// itself tracks the cursor, sibling rows shift around it continuously, and
/// the gesture's `onEnded` is the one deterministic end-of-drag signal
/// (releases, including outside the pane, always deliver it).
private struct ActiveQueueDrag {
    let lane: QueueLaneID
    let entryId: String
    let trackId: String
    /// The dragged row's canonical index in its lane at drag start (display
    /// order equals canonical order at that moment — no permutation is live).
    let startSlot: Int
    /// Cursor offset from the row's top at grab time, so the row keeps its
    /// grip point under the cursor instead of snapping its top edge there.
    let grabAnchorY: CGFloat
    /// Cursor position, pane space. Updated continuously by the gesture.
    var location: CGPoint
}

/// Shared drag state for both `QueueSection`s: the active gesture, each
/// lane's measured geometry, and the post-commit hold. All display effects
/// (sibling shifts, the dragged row's cursor tracking, the cross-lane
/// insertion gap) are DERIVED from `active` + geometry per render — there is
/// no stored permutation to go stale when the queue changes mid-drag.
@MainActor
@Observable
final class QueueDragCoordinator {
    private var active: ActiveQueueDrag?
    private var geometry: [QueueLaneID: QueueLaneGeometry] = [:]
    /// A committed reorder's final display order, held per lane so the rows
    /// stay put between the gesture ending and core's snapshot echoing the
    /// same order back (the revision bump clears it — visually a no-op).
    private var hold: [QueueLaneID: [Int]] = [:]

    var isDragging: Bool { active != nil }

    /// Whether `lane` hosts the live drag — its section must also win the
    /// z-order between SECTIONS, or a row dragged across the boundary renders
    /// underneath the sibling section's rows (zIndex only arbitrates among
    /// siblings, and the sections are siblings of each other).
    func isDragSource(_ lane: QueueLaneID) -> Bool {
        active?.lane == lane
    }

    /// Whether the live drag is a context row currently aimed at the manual
    /// lane. While it is, the dragged row sits back home (dimmed) and the
    /// manual lane's insertion line is the ONE target indicator — a floating
    /// row and a line drawn through each other read as broken.
    var isCrossLaneTargeting: Bool {
        manualInsertGap(manualCount: manualGapCount) != nil
    }

    /// The dragged entry while a drag is live in `lane`, else `nil`.
    func draggedEntryId(in lane: QueueLaneID) -> String? {
        guard let active, active.lane == lane else {
            return nil
        }
        return active.entryId
    }

    func setRowsGeometry(_ lane: QueueLaneID, frame: CGRect, rowCount: Int) {
        var geo = geometry[lane] ?? QueueLaneGeometry()
        geo.rowsFrame = frame
        geo.rowHeight = rowCount > 0 ? frame.height / CGFloat(rowCount) : 0
        if geometry[lane]?.rowsFrame != geo.rowsFrame
            || geometry[lane]?.rowHeight != geo.rowHeight
        {
            geometry[lane] = geo
        }
    }

    func setAppendFrame(_ lane: QueueLaneID, frame: CGRect) {
        var geo = geometry[lane] ?? QueueLaneGeometry()
        geo.appendFrame = frame
        if geometry[lane]?.appendFrame != geo.appendFrame {
            geometry[lane] = geo
        }
    }

    func begin(
        lane: QueueLaneID,
        entryId: String,
        trackId: String,
        startSlot: Int,
        location: CGPoint
    ) {
        guard active == nil else {
            return
        }
        let rowTop: CGFloat
        if let geo = geometry[lane] {
            rowTop = geo.rowsFrame.minY + CGFloat(startSlot) * geo.rowHeight
        }
        else {
            rowTop = location.y
        }
        active = ActiveQueueDrag(
            lane: lane,
            entryId: entryId,
            trackId: trackId,
            startSlot: startSlot,
            grabAnchorY: location.y - rowTop,
            location: location
        )
    }

    func update(location: CGPoint) {
        active?.location = location
    }

    /// The display slot the dragged row currently occupies in its own lane:
    /// the slot under the cursor, clamped to the lane. Sibling rows shift
    /// around it as this crosses row boundaries — continuously, not on
    /// enter/leave events.
    func gapSlot(count: Int) -> Int? {
        guard let active, let geo = geometry[active.lane],
            geo.rowHeight > 0,
            active.startSlot < count
        else {
            return nil
        }
        let rel =
            (active.location.y - active.grabAnchorY + geo.rowHeight / 2
                - geo.rowsFrame.minY) / geo.rowHeight
        return min(max(Int(rel.rounded(.down)), 0), count - 1)
    }

    /// The lane's current display order (display slot → canonical index):
    /// the live permutation while a drag is underway here, the held
    /// post-commit order while core's echo is in flight, else canonical.
    /// Derived against the CURRENT `count`, so a queue change mid-drag can
    /// never leave a stale-length permutation — the drag simply falls back
    /// to canonical if its start slot no longer exists.
    func displayOrder(for lane: QueueLaneID, count: Int) -> [Int]? {
        if let active, active.lane == lane {
            guard !isCrossLaneTargeting,
                let gap = gapSlot(count: count), gap != active.startSlot
            else {
                return nil
            }
            var order = Array(0..<count)
            order.remove(at: active.startSlot)
            order.insert(active.startSlot, at: gap)
            return order
        }
        if let held = hold[lane], held.count == count {
            return held
        }
        return nil
    }

    /// The dragged row's vertical offset from its current display slot, so it
    /// tracks the cursor exactly while siblings animate around it.
    func draggedRowOffset(displaySlot: Int) -> CGFloat {
        guard let active, let geo = geometry[active.lane] else {
            return 0
        }
        let slotTop =
            geo.rowsFrame.minY + CGFloat(displaySlot) * geo.rowHeight
        return active.location.y - active.grabAnchorY - slotTop
    }

    /// Where a context-row drag currently hovers in the MANUAL lane: the
    /// between-row gap (0...count) its track would insert at, or `nil` when
    /// the drag isn't a context row over the manual lane. The manual section
    /// renders its insertion line here; releasing commits the enqueue.
    func manualInsertGap(manualCount: Int) -> Int? {
        guard let active, active.lane == .context,
            let manual = geometry[.manual]
        else {
            return nil
        }
        // Over the rows: the nearest between-row gap. Past them (or when the
        // lane has no rows at all — the whole lane is then just its trailing
        // zone), the append gap at the lane's end.
        if manual.rowHeight > 0, active.location.y < manual.rowsFrame.maxY {
            let rel =
                (active.location.y - manual.rowsFrame.minY) / manual.rowHeight
            return min(max(Int(rel.rounded()), 0), manualCount)
        }
        if manual.appendFrame.contains(
            CGPoint(x: manual.appendFrame.midX, y: active.location.y)
        ) {
            return manualCount
        }
        return nil
    }

    /// End the drag, resolving what (if anything) to commit. The caller (the
    /// source lane's section, which owns the command callbacks and `itemAt`)
    /// executes the outcome; `reorder` holds the final order here until the
    /// revision bump.
    func finish(count: Int) -> Outcome {
        guard let active else {
            return .none
        }
        defer { self.active = nil }
        if let gap = manualInsertGap(manualCount: manualGapCount) {
            return .insertIntoManual(trackId: active.trackId, gap: gap)
        }
        guard let order = displayOrder(for: active.lane, count: count),
            let gap = gapSlot(count: count)
        else {
            return .none
        }
        return .reorder(
            entryId: active.entryId,
            finalOrder: order,
            gap: gap,
            lane: active.lane
        )
    }

    /// Keep a committed reorder's order on screen until core echoes it back.
    func holdOrder(_ order: [Int], lane: QueueLaneID) {
        hold[lane] = order
    }

    /// Core's snapshot landed for this lane's revision: the canonical order
    /// now equals whatever was held, so dropping the hold is visually a no-op.
    func clearHold(_ lane: QueueLaneID) {
        hold[lane] = nil
    }

    /// The manual lane's row count, captured for cross-lane gap math (the
    /// context section's gesture can't see the manual section's props).
    var manualGapCount = 0

    enum Outcome {
        case none
        case reorder(
            entryId: String,
            finalOrder: [Int],
            gap: Int,
            lane: QueueLaneID
        )
        case insertIntoManual(trackId: String, gap: Int)
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

/// A row's identity in the `ForEach` below: a loaded row is keyed by its entry
/// id (stable across a live reorder, which is what lets SwiftUI animate the
/// move); an unloaded row is keyed by its display slot instead, since it has no
/// content of its own to identify. `sourceIndex` is the absolute, canonical
/// index `itemAt`/`loadRange` address — it equals `displaySlot` outside a live
/// reorder, and diverges from it only while `QueueSection.liveOrder` is active.
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
private struct QueueSection: View {
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
    /// reorder this lane just committed — so `liveOrder` is dropped: the
    /// canonical order the new snapshot carries already equals what was on
    /// screen, so clearing it is visually a no-op, not a snap.
    let queueRevision: UInt64
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
    /// Rows removed optimistically: the X collapses the row on the spot, the
    /// remove command races the snapshot behind the animation, and the
    /// revision bump (whose canonical order no longer carries the entry)
    /// clears the set. The alternative — waiting for the round trip — reads
    /// as the click not registering.
    @State
    private var removingEntryIds: Set<String> = []

    /// Display slots 0..<count, each resolved to its source index (identity
    /// under `liveOrder`, or permuted while a drag is live) and an identity —
    /// the entry id when loaded, a slot sentinel when not. A `LazyMapCollection`
    /// rather than a materialized array: `itemAt` is only ever called for rows
    /// `ForEach`/`LazyVStack` actually need (visible rows plus a small buffer),
    /// preserving the windowed-loading contract's whole point for a
    /// library-scaled context lane — building an eager `[QueueRowSlot]` here
    /// would call `itemAt` for every index up front regardless of what's
    /// on screen.
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
                    VStack(spacing: 0) {
                        queueRow(item, index: slot.displaySlot)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 2)
                        // An explicit 1pt rule, not `Divider()`: macOS gives
                        // Divider ~10pt of layout with the hairline centered,
                        // which padded every row out and left the visible
                        // border 5pt above the row's edge — so the insertion
                        // line overlay (pinned to the edge) read as sitting
                        // inside the item.
                        Rectangle()
                            .fill(.white.opacity(0.08))
                            .frame(height: 1)
                            .padding(.leading, 62)
                    }
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
                    // Anchored to the BOTTOM of the row above the gap — the
                    // same edge the row's 1pt rule renders on — so the line
                    // draws over that rule; a top-anchored line on the row
                    // below kept landing visibly under it. Gap 0 (above the
                    // first row) is the one gap with no row above; it anchors
                    // to row 0's top instead.
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
                            slot.sourceIndex - queueUpcomingLoadBatchSize / 2
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
        // A drag started: drop the stored hover slot. `.onHover` won't fire
        // again until the gesture ends, so without this the pre-drag value
        // would resurface on whichever row holds that slot afterwards.
        .onChange(of: coordinator.isDragging) {
            if coordinator.isDragging {
                hoveredIndex = nil
            }
        }
    }

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
        if title != nil || onSetShuffle != nil {
            HStack(spacing: 6) {
                if let title {
                    Text(title)
                        .font(.caption)
                        .fontWeight(.semibold)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if let onSetShuffle {
                    shuffleToggle(onSetShuffle)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 4)
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
                .font(.caption2)
                .foregroundStyle(shuffled ? Color.accentColor : .secondary)
                // Same 28pt slot as the rows' X column, so the glyphs align
                // vertically — and the same comfortable hit target.
                .frame(width: 28, height: 28)
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
                    // Hover chrome is suppressed for the whole drag: the
                    // pre-drag `hoveredIndex` goes stale once rows start
                    // shuffling — without the guard, whatever row holds that
                    // display slot wears the remove/play chrome mid-drag.
                    isHovered: hoveredIndex == index
                        && !coordinator.isDragging,
                    onHoverChanged: { hovering in
                        guard !coordinator.isDragging else {
                            return
                        }
                        hoveredIndex = hovering ? index : nil
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

/// One loaded row: cover art (with a hover play overlay), title/album, and a
/// duration that swaps for a remove button on hover. Broken out of
/// `QueueSection` (which otherwise owns every row's chrome — drop targets,
/// insertion line, load hook) purely to keep that type's body a reasonable
/// size; it has no state of its own; hover is the section's (only one row is
/// ever hovered at a time) so it flows in and back out via `isHovered`/
/// `onHoverChanged`.
private struct QueueItemRow: View {
    let item: QueueItem
    let isHovered: Bool
    let onHoverChanged: (Bool) -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void

    /// The remove X's own hover, distinct from the row's: it backs the ring
    /// that marks the control as live before any press.
    @State
    private var removeHovered = false

    var body: some View {
        HStack(spacing: 10) {
            artWithHoverOverlay

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

            // Duration and the remove X coexist — no hover swap: the swap
            // needed hover-state plumbing (and broke when rows slid under a
            // stationary pointer) for no real estate gain.
            Text(item.durationLabel)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            Button(action: { onRemove(item.id) }) {
                Image(systemName: "xmark")
                    .font(.caption)
                    .foregroundStyle(removeHovered ? .primary : .secondary)
                    // Faint at rest, bright on hover: always present and
                    // hittable (a hover-revealed X can't reveal itself on a
                    // row that slides under a stationary pointer), without
                    // reading as a column of controls.
                    .opacity(removeHovered ? 1 : 0.4)
                    // Same small glyph, comfortable click target.
                    .frame(width: 28, height: 28)
                    .background(
                        Circle()
                            .fill(.white.opacity(removeHovered ? 0.12 : 0))
                            .frame(width: 20, height: 20)
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(PressableIconButtonStyle())
            .onHover { removeHovered = $0 }
            .help("Remove from queue")
        }
        .contentShape(Rectangle())
        .onHover(perform: onHoverChanged)
        .onTapGesture(count: 2) {
            onSkipTo(item.id)
        }
        .contextMenu {
            Button("Remove from Queue") {
                onRemove(item.id)
            }
        }
    }

    // The hover play overlay stays in the tree and toggles by opacity/hit-testing
    // so revealing it on hover doesn't resize the row and re-lay-out the lane.
    private var artWithHoverOverlay: some View {
        ZStack {
            ImageView(coverImageId: item.coverImageId, pointSize: 40)
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
                    // The whole hovered cover is the target, not the glyph.
                    .frame(width: 40, height: 40)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(isHovered ? 1 : 0)
            .allowsHitTesting(isHovered)
        }
    }
}

/// A not-yet-loaded row: a skeleton shape, no text — `loadRange` is already in
/// flight for it via the row's `.task(id:)`.
private struct QueuePlaceholderRow: View {
    var body: some View {
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
}

// MARK: - Drop Delegate

/// External album-card drops into the manual lane — the one drag interaction
/// still on the OS drag machinery, because it genuinely crosses views (the
/// library grid to this pane). Internal reorders and cross-lane enqueues are
/// gesture-driven in `QueueSection` and never reach this delegate.
private struct QueueDropDelegate: DropDelegate {
    /// The insertion gap this target addresses (`targetIndex == count` is the
    /// trailing append zone, past the last row).
    let targetIndex: Int
    /// Whether external track drops are accepted into this lane. The manual
    /// lane inserts them; the context lane (the release's own order) refuses.
    let acceptsExternalDrops: Bool
    @Binding
    var dropInsertIndex: Int?
    let onInsertTracks: ([String], Int) -> Void

    func dropEntered(info _: DropInfo) {
        if acceptsExternalDrops {
            dropInsertIndex = targetIndex
        }
    }

    func dropUpdated(info _: DropInfo) -> DropProposal? {
        DropProposal(operation: acceptsExternalDrops ? .copy : .forbidden)
    }

    func performDrop(info: DropInfo) -> Bool {
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
        acceptsExternalDrops
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
