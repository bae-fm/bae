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

    // A drag can start in either section; the session is shared so the source
    // row dims wherever it lives and both sections revert on the same cancel.
    // (Hover and drop-insertion are positional, so each QueueSection still owns
    // its own — a row index in one lane must not light up the same index in the
    // other.)
    @State
    private var dragSession = QueueDragSession()

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
                            dragSession: dragSession,
                            queueRevision: playbackStore.revision,
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
                                dragSession: dragSession,
                                queueRevision: playbackStore.revision,
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

// MARK: - Drag session

/// The in-flight internal-reorder drag, shared across both `QueueSection`s.
/// Reference-typed (not `@State` value fields) so the cancel watchdog below can
/// mutate it from its polling task.
///
/// Neither `.onDrag` nor any `DropDelegate` reports a drag that ends WITHOUT a
/// drop on one of our targets — Esc, or a release outside the pane. Hanging
/// cleanup off the drag `NSItemProvider`'s release doesn't work either: the
/// drag pasteboard can retain the provider until the NEXT drag begins, so a
/// cancelled drag's ghost row would linger indefinitely. The one reliable,
/// bounded signal left is the mouse button itself: while a session is live, a
/// watchdog polls `NSEvent.pressedMouseButtons`; a few consecutive
/// buttons-up ticks with no committed drop means the drag ended un-dropped —
/// cancel. The debounce exists so an accepted drop's `performDrop` (dispatched
/// within one runloop of release) always wins the race against the watchdog.
@MainActor
@Observable
final class QueueDragSession {
    /// The dragged row's entry id while an internal drag is live, else `nil`.
    /// Drives the source row's dimmed opacity and lets each drop delegate tell
    /// an internal reorder from a foreign drag.
    var draggedEntryId: String?
    /// The dragged row's underlying track, for a cross-lane drop: a context row
    /// released over the manual lane enqueues the track (the context instance
    /// stays — the release's order isn't edited by promoting a track into
    /// "Up Next").
    var draggedTrackId: String?
    /// Set by the row drop delegate when an internal reorder commits (which
    /// also un-dims immediately — the drop's success is known synchronously
    /// there). The watchdog reads it to decide: a committed move leaves the
    /// live permutation on screen for the coming revision bump to clear (no
    /// snap); anything else — Esc, drag-out, drop-on-chrome — reverts.
    var committed = false
    /// Bumped when a drag ends WITHOUT a commit, so each `QueueSection` animates
    /// its live permutation back to canonical order. Separate from the revision
    /// bump, which clears a committed permutation un-animated.
    private(set) var cancelTick = 0
    /// Identifies the current drag, so a superseded watchdog tick can never
    /// revert or un-dim a newer drag.
    private var sessionId = 0
    private var watchdog: Task<Void, Never>?

    func begin(entryId: String, trackId: String) {
        sessionId += 1
        draggedEntryId = entryId
        draggedTrackId = trackId
        committed = false

        let id = sessionId
        watchdog?.cancel()
        watchdog = Task { [weak self] in
            // 3 consecutive buttons-up ticks ≈ 300ms after physical release:
            // long past when an accepted drop's performDrop lands (same
            // runloop-ish), short enough that a cancel's revert reads as part
            // of AppKit's own slide-back animation. If the main thread is
            // stalled long enough to reorder the two, the wrongly-reverted
            // visuals still correct on the reorder's snapshot — core stays
            // authoritative either way.
            var releasedTicks = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(100))
                guard let self, self.sessionId == id,
                    self.draggedEntryId != nil
                else {
                    return
                }
                if NSEvent.pressedMouseButtons == 0 {
                    releasedTicks += 1
                    if releasedTicks >= 3 {
                        self.end(sessionId: id)
                        return
                    }
                }
                else {
                    releasedTicks = 0
                }
            }
        }
    }

    /// The drag identified by `sessionId` ended un-dropped (the watchdog saw
    /// the mouse released with no commit). Reverts the live permutation unless
    /// a commit already claimed it. A no-op for a superseded session.
    func end(sessionId: Int) {
        guard sessionId == self.sessionId else {
            return
        }
        // Animated: the source row's un-dim (and, on the cancel path, the
        // sections' permutation revert keyed off `cancelTick`) should fade in
        // step with AppKit's own slide-back, not pop.
        withAnimation(.snappy) {
            if !committed {
                cancelTick += 1
            }
            draggedEntryId = nil
            draggedTrackId = nil
        }
        committed = false
        watchdog?.cancel()
        watchdog = nil
    }

    /// A cross-lane drop committed (context row enqueued into the manual
    /// lane). Core inserted a NEW manual entry — the source lane's own order
    /// never changed — so unlike a same-lane commit, any live permutation in
    /// the source section must revert (animated, via `cancelTick`), while the
    /// insert itself arrives with the coming snapshot.
    func endAsCrossLaneInsert() {
        withAnimation(.snappy) {
            cancelTick += 1
            draggedEntryId = nil
            draggedTrackId = nil
        }
        committed = false
        watchdog?.cancel()
        watchdog = nil
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
    /// The shared in-flight drag: source-row identity for dimming, and the
    /// `cancelTick` each section watches to revert on a cancelled drag.
    let dragSession: QueueDragSession
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
    /// Display position → source index, active only while an internal drag in
    /// this lane is live. `nil` means canonical order. Read live by every row's
    /// identity and content (`rowSlots`) and by the drop delegates that mutate
    /// it as the drag crosses row boundaries; written back to canonical (`nil`)
    /// on drop-commit (via `queueRevision`) or on cancel (via
    /// `dragSession.cancelTick`).
    @State
    private var liveOrder: [Int]?

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
        // Use the live permutation only while it still matches `count`. A
        // `QueueUpdated` landing mid-drag (a lane grows — sync from another
        // device, Play Next — or shrinks) resizes `count` for the one body pass
        // before `.onChange(of: queueRevision)` clears `liveOrder`. On grow,
        // `liveOrder[displaySlot]` would read past the stale-short array's end;
        // on shrink, its indices point past the new bounds and resolve to
        // stale positions. A length mismatch falls back to canonical order for
        // that pass — `sourceIndex == displaySlot`, in range, and any index the
        // shorter data can't resolve renders a placeholder.
        let order = liveOrder?.count == count ? liveOrder : nil
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

            ForEach(rowSlots) { slot in
                let item = itemAt(slot.sourceIndex)
                VStack(spacing: 0) {
                    // Only the manual lane ever sets `dropInsertIndex` (external
                    // drops land there only) — an internal reorder moves rows
                    // live instead of showing an insertion line, so the line is
                    // omitted entirely on the context lane, which never uses it.
                    if acceptsExternalDrops {
                        insertionLine
                            .opacity(
                                dropInsertIndex == slot.displaySlot ? 1 : 0
                            )
                            .allowsHitTesting(false)
                    }
                    queueRow(item, index: slot.displaySlot)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 4)
                        .opacity(
                            item != nil
                                && dragSession.draggedEntryId == item?.id
                                ? 0.3 : 1.0
                        )
                    Divider().padding(.leading, 62)
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
                        targetLoaded: item != nil,
                        count: count,
                        itemAt: itemAt,
                        acceptsExternalDrops: acceptsExternalDrops,
                        dragSession: dragSession,
                        liveOrder: $liveOrder,
                        dropInsertIndex: $dropInsertIndex,
                        onReorder: onReorder,
                        onInsertTracks: onInsertTracks,
                    )
                )
            }
            // The trailing drop line (insert at the lane's end) stays in the tree
            // and toggles by opacity, matching the per-row lines above — manual
            // lane only, for the same reason as the per-row line above.
            if acceptsExternalDrops {
                insertionLine
                    .opacity(dropInsertIndex == count ? 1 : 0)
                    .allowsHitTesting(false)
            }

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
                        dragSession: dragSession,
                        liveOrder: $liveOrder,
                        dropInsertIndex: $dropInsertIndex,
                        onReorder: onReorder,
                        onInsertTracks: onInsertTracks,
                    )
                )
        }
        // The drop just committed: core's next snapshot carries the same order
        // already displayed, so dropping the permutation here is a no-op, not
        // an animated snap.
        .onChange(of: queueRevision) {
            liveOrder = nil
        }
        // The drag ended without a commit (drop-on-chrome, drag-out, or Esc):
        // the displayed order was never committed, so reverting it is a real,
        // visible change — animate it.
        .onChange(of: dragSession.cancelTick) {
            guard liveOrder != nil else {
                return
            }
            withAnimation(.snappy) {
                liveOrder = nil
            }
        }
        // A drag started: drop the stored hover slot. `.onHover` won't fire
        // again until the session ends, so without this the pre-drag value
        // would resurface on whichever row holds that slot once the session's
        // `draggedEntryId` clears.
        .onChange(of: dragSession.draggedEntryId) {
            if dragSession.draggedEntryId != nil {
                hoveredIndex = nil
            }
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

    /// A row at `index`: the resolved item, or a placeholder while it loads.
    private func queueRow(_ item: QueueItem?, index: Int) -> some View {
        Group {
            if let item {
                QueueItemRow(
                    item: item,
                    // Hover chrome is suppressed for the whole drag session:
                    // `.onHover` doesn't fire while a drag is live, so the
                    // pre-drag `hoveredIndex` goes stale — without the guard,
                    // whatever row shuffles into that display slot wears the
                    // remove/play chrome throughout the drag and at the drop.
                    isHovered: hoveredIndex == index
                        && dragSession.draggedEntryId == nil,
                    onHoverChanged: { hovering in
                        guard dragSession.draggedEntryId == nil else {
                            return
                        }
                        hoveredIndex = hovering ? index : nil
                    },
                    onDragStarted: {
                        dragSession.begin(
                            entryId: item.id,
                            trackId: item.trackId
                        )
                    },
                    onSkipTo: onSkipTo,
                    onRemove: onRemove
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
    /// The section records this row as the drag's source (which also arms the
    /// session's cancel watchdog); the `NSItemProvider` carrying `item.id` is
    /// what the drop side reads back.
    let onDragStarted: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void

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

            // Hover swaps the duration for a remove button. Both stay in the tree
            // and toggle by opacity/hit-testing so the swap doesn't resize the row
            // and re-lay-out the lane. (The trailing slot is the wider of the two,
            // so neither toggle changes the row's intrinsic width.)
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
        .onHover(perform: onHoverChanged)
        .onDrag {
            onDragStarted()
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

private struct QueueDropDelegate: DropDelegate {
    /// This row's display position — a slot in `liveOrder`, not the underlying
    /// item's canonical index (`targetIndex == count` addresses the trailing
    /// append zone, past the last row).
    let targetIndex: Int
    /// Whether the item currently displayed at `targetIndex` is loaded. An
    /// internal reorder onto an unloaded row is rejected — `onReorder` needs
    /// the target's own entry id (core reorders by id, not index), which isn't
    /// known yet. In practice a row must be visible (and its load task already
    /// running) to receive a drop at all, so this only ever excludes a brief
    /// loading flash, not a real position. Always `true` for the trailing zone,
    /// which has no row of its own to be unloaded. The same "needs a loaded
    /// entry id to pin against" rule applies to the *anchor* row a committed
    /// drop lands before — see `performDrop`, which refuses a drop whose
    /// next-displayed row exists but hasn't loaded.
    let targetLoaded: Bool
    let count: Int
    let itemAt: (Int) -> QueueItem?
    /// Whether external (non-row) track drops are accepted into this lane. The
    /// manual lane inserts them; the context lane ignores them (you can't add a
    /// track into the release's own order) and only handles internal reorders.
    let acceptsExternalDrops: Bool
    /// The shared in-flight drag — its `draggedEntryId` identifies the source
    /// row; `committed` is set here on a successful drop so the session-end
    /// signal leaves the permutation for the revision bump instead of reverting.
    let dragSession: QueueDragSession
    /// The lane's live display-order permutation — see
    /// `QueueSection.liveOrder`.
    @Binding
    var liveOrder: [Int]?
    @Binding
    var dropInsertIndex: Int?
    /// Move the dragged entry to sit before `beforeEntryId`; `nil` = end.
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void

    /// The dragged entry's canonical (source) index in this lane, or `nil` if
    /// it isn't dragging from this lane. Resolved against `itemAt` (canonical
    /// order), never against `liveOrder` — the entry's true position doesn't
    /// change until core commits the reorder.
    private var fromIndex: Int? {
        guard let draggedId = dragSession.draggedEntryId else {
            return nil
        }
        return (0..<count).first { itemAt($0)?.id == draggedId }
    }

    /// Whether this is an internal reorder within this lane (the dragged entry
    /// is one of this lane's rows). A queue drag from the OTHER lane is a
    /// cross-lane insert instead: dropping a context row on the manual lane
    /// enqueues its track (`dragSession.draggedTrackId`) — the release's own
    /// order can't take foreign rows, so the reverse direction stays forbidden.
    private var isInternalDrag: Bool {
        fromIndex != nil
    }

    /// A live queue drag that started in the other lane.
    private var isCrossLaneDrag: Bool {
        dragSession.draggedEntryId != nil && fromIndex == nil
    }

    /// The live permutation, but only while it still matches the current row
    /// count. A `QueueUpdated` landing mid-drag resizes `count` for a beat
    /// before `.onChange(of: queueRevision)` clears `liveOrder`; a stale-length
    /// permutation would subscript out of range (lane grew) or address stale
    /// positions (lane shrank), so any length mismatch reads as canonical order.
    /// Mirrors the identical guard in `QueueSection.rowSlots`.
    private var activeOrder: [Int]? {
        guard let liveOrder, liveOrder.count == count else {
            return nil
        }
        return liveOrder
    }

    func dropEntered(info _: DropInfo) {
        if let fromIndex {
            // Internal reorder: live-move the dragged entry to this row's
            // display position. A target that isn't loaded is a no-op — core
            // reorders by id, and an unloaded row's id isn't known yet, so the
            // permutation is left exactly as it was.
            guard targetLoaded else {
                return
            }
            var order = activeOrder ?? Array(0..<count)
            guard let currentSlot = order.firstIndex(of: fromIndex),
                currentSlot != targetIndex
            else {
                return
            }
            let moved = order.remove(at: currentSlot)
            let insertAt =
                targetIndex > currentSlot ? targetIndex - 1 : targetIndex
            order.insert(moved, at: insertAt)
            // Near-interactive: the default `.snappy` (~0.3s) is still mid-
            // flight when a drop lands right after crossing a row boundary,
            // and that residual slide reads as the drop lagging the release.
            withAnimation(.snappy(duration: 0.15, extraBounce: 0)) {
                liveOrder = order
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
        if let draggedId = dragSession.draggedEntryId, let fromIndex,
            targetLoaded
        {
            // The drop commits exactly what dropEntered already put on screen:
            // the dragged entry lands before whatever now sits right after it in
            // the live order.
            let order = activeOrder ?? Array(0..<count)
            guard let slot = order.firstIndex(of: fromIndex), slot != fromIndex
            else {
                // Dropped back at its own slot (no rows crossed): the
                // permutation already equals canonical, so there's nothing to
                // commit. Leave `committed` false — the watchdog's revert is a
                // visual no-op that also clears the stray identity
                // permutation. Un-dim now (instantly, under the dissolving
                // drag image): the drop is done.
                dragSession.draggedEntryId = nil
                return true
            }

            // The row after the dragged entry is the reorder's anchor. Nothing
            // after it is a genuine lane end (commit `nil` = "to the end"). But
            // a row that EXISTS yet isn't loaded has no entry id to pin before —
            // and core reads a `nil` anchor as lane-end, so committing here
            // would snap the entry to the bottom on the next snapshot (the same
            // reason `targetLoaded` gates the drop target itself). Refuse it:
            // skip `onReorder` and leave the session live with `committed`
            // false — the watchdog sees the mouse released without a commit
            // and reverts the permutation (and the dim) with animation.
            if slot + 1 < order.count {
                guard let beforeEntryId = itemAt(order[slot + 1])?.id else {
                    return false
                }
                onReorder(draggedId, beforeEntryId)
            }
            else {
                onReorder(draggedId, nil)
            }

            // Mark the session committed so the watchdog leaves `liveOrder` in
            // place — it already equals what core will echo back in the next
            // snapshot, and `QueueSection` drops it on that revision bump.
            // Un-dim right here, INSTANTLY: the drop's success is known
            // synchronously, AppKit's dissolving drag image sits exactly over
            // the row, and a fade under it reads as the drop lagging the
            // release. Nothing about the drag machinery (provider release,
            // watchdog tick) is allowed to delay an accepted drop's settle.
            dragSession.committed = true
            dragSession.draggedEntryId = nil
            return true
        }

        // A queue row dragged from the other lane: enqueue its track at this
        // position (manual lane only — the context is the release's own order
        // and takes no foreign rows). The context instance stays; core allows
        // duplicate instances by design, so the track plays here AND when the
        // context reaches it, matching how an album-card drop behaves.
        if isCrossLaneDrag {
            defer { dropInsertIndex = nil }
            guard acceptsExternalDrops,
                let trackId = dragSession.draggedTrackId
            else {
                return false
            }
            onInsertTracks([trackId], targetIndex)
            dragSession.endAsCrossLaneInsert()
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
