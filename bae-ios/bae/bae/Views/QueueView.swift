import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("Queue")

/// Range fetched around an unloaded context row as the queue scrolls, mirroring
/// the page size `PlaybackStore.loadUpcomingRange`'s bridge call uses.
private let queueUpcomingLoadBatchSize = 100

// periphery:ignore
/// Row load identity: the store's queue revision (so a bump restarts every
/// visible row's load task) plus the row's absolute index. The manual lane,
/// always fully loaded, passes a constant epoch and a `nil` load hook so its
/// rows never fire a load.
private struct QueueRowLoadID: Hashable {
    let epoch: UInt64
    let index: Int
}

/// One queue lane's row-addressing and on-demand loading: how many rows it
/// spans, how to resolve the item at an absolute index (`nil` when not yet
/// loaded), the load epoch that restarts a row's fetch task when the queue
/// revision bumps, and the range fetch for unloaded rows — `nil` for the
/// always-fully-loaded manual lane, which never pages. The context lane fills
/// these from its windowed store; the manual lane from its in-memory array.
struct QueueLane {
    let count: Int
    let itemAt: (Int) -> QueueItem?
    let loadEpoch: UInt64
    let loadRange: ((_ offset: Int, _ limit: Int) async -> Void)?
}

/// The play queue, presented as a sheet: the currently-playing track, then a
/// reorderable "Up Next" list. Reads the authoritative now-playing and queue
/// off the shared `PlaybackStore`; mutations go straight to the `Queue` service
/// (`clearQueue` / `removeEntry` / `reorderEntry` / `skipToEntry`) and reflect
/// back as a `QueueUpdated` event that re-populates `queueItems`.
///
/// The view iterates and renders only — `durationLabel` is pre-formatted and the
/// queue arrives pre-ordered. The current track is never in `queueItems`; it
/// lives in `nowPlaying`.
struct QueueView: View {
    @Environment(Queue.self)
    private var queue
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if let track = playbackStore.nowPlaying.track {
                    Section("Now Playing") {
                        NowPlayingRow(track: track)
                    }
                }

                upNext
                playingFrom
            }
            .listStyle(.plain)
            .background(Theme.background)
            .navigationTitle("Queue")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    // Clear empties only the manual lane; the context (the release
                    // being played from) survives, so it disables on an empty
                    // manual lane regardless of the context.
                    Button("Clear") { queue.clearQueue() }
                        .disabled(playbackStore.manualQueue.isEmpty)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    @ViewBuilder
    private var upNext: some View {
        if playbackStore.manualQueue.isEmpty {
            // Only show the empty message when nothing follows at all — no manual
            // lane and no context section below. With a context present, the
            // "Playing From" section carries the queue, so no message is needed.
            if playbackStore.queueContext == nil {
                Section {
                    Text(
                        playbackStore.nowPlaying.track == nil
                            ? String(localized: "Queue is empty")
                            : String(localized: "Nothing up next")
                    )
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 24)
                }
            }
        }
        else {
            Section("Up Next") {
                // Reorder handles are always on (see `upNextRows`); a tap always
                // skips, and skipping dismisses the sheet. Always fully resolved,
                // so no load hook.
                let manual = playbackStore.manualQueue
                upNextRows(
                    lane: QueueLane(
                        count: manual.count,
                        itemAt: { manual.indices.contains($0) ? manual[$0] : nil },
                        loadEpoch: 0,
                        loadRange: nil
                    ),
                    queue: queue,
                    onSkipped: { dismiss() }
                )
            }
        }
    }

    // The context (the release being played from): its not-yet-played tail, with
    // a shuffle toggle in the header that flips its order while the current track
    // keeps playing. The rows skip/remove/reorder by entry id, the same as the
    // manual lane. The tail is library-scaled and only partly resolved; unloaded
    // rows show a placeholder and trigger `loadUpcomingRange`.
    @ViewBuilder
    private var playingFrom: some View {
        if let context = playbackStore.queueContext, context.upcomingTotal > 0 {
            Section {
                upNextRows(
                    lane: QueueLane(
                        count: context.upcomingTotal,
                        itemAt: { playbackStore.upcomingItem(at: $0) },
                        loadEpoch: playbackStore.revision,
                        loadRange: { offset, limit in
                            await playbackStore.loadUpcomingRange(
                                offset: offset,
                                limit: limit,
                                queue: queue
                            )
                        }
                    ),
                    queue: queue,
                    onSkipped: { dismiss() }
                )
            } header: {
                playingFromHeader(
                    kind: context.kind,
                    shuffled: context.shuffled,
                    queue: queue
                )
            }
        }
    }
}

/// The context-section header with its shuffle toggle — shared by the queue
/// sheet and the expanded player's embedded queue so the control can't drift.
/// The title names what's playing — a release ("Playing From") vs the whole
/// library — by `kind`. The toggle is tinted when on; tapping it flips the
/// context's order while the current track keeps playing.
@MainActor
@ViewBuilder
func playingFromHeader(
    kind: BridgePlaybackSourceKind,
    shuffled: Bool,
    queue: Queue
) -> some View {
    HStack(spacing: 6) {
        Text(contextSectionTitle(kind))
        Spacer()
        Button {
            queue.setShuffle(!shuffled)
        } label: {
            Image(systemName: "shuffle")
                .foregroundStyle(shuffled ? Theme.accent : .secondary)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(
            shuffled ? Text("Turn off shuffle") : Text("Shuffle")
        )
    }
}

/// The context section's title, by what it plays from: a release keeps the
/// "Playing From" label; the library names itself. Resolving a localized key by
/// the source kind is the UI's locale-rendering job.
func contextSectionTitle(_ kind: BridgePlaybackSourceKind) -> LocalizedStringKey {
    switch kind {
    case .release:
        return "Playing From"
    case .library:
        return "Your Library"
    }
}

/// The "Up Next" rows — shared by the queue sheet and the expanded player's
/// embedded queue so the index conventions can't drift. Addressed by absolute
/// index via `itemAt`, not a concrete array, so the context lane can be a
/// library-scaled, partly-loaded tail: an unloaded index renders a placeholder
/// and `.task(id:)` fetches the range around it via `loadRange` (`nil` for the
/// always-fully-loaded manual lane). Tapping a loaded row always skips to it and
/// runs `onSkipped`; swiping a loaded row deletes it; `.onMove` reorders.
///
/// Reorder handles are always visible in both surfaces — no `EditButton`, no
/// edit-mode toggle — matching Apple Music's Up Next. That needs
/// `EditMode.active` scoped to just this `ForEach` (`.onMove` is inert
/// without it), which would normally also swap every row's trailing swipe
/// gesture for the edit-mode delete (minus) control; using `.swipeActions`
/// instead of `.onDelete` keeps the delete affordance a real swipe regardless
/// of edit mode (and, since it's attached only when `item` is loaded, an
/// unloaded row is simply not swipeable rather than swiping and silently
/// no-op'ing). The now-playing row and the section headers sit outside this
/// `ForEach`, so the constant edit mode never reaches them.
///
/// A reorder whose source or target row isn't loaded is dropped with a
/// warning rather than guessed — reordering only ever targets a rendered
/// (hence loaded) row in practice, so this only ever excludes a brief loading
/// flash, not a real position.
@MainActor
@ViewBuilder
func upNextRows(
    lane: QueueLane,
    queue: Queue,
    onSkipped: @escaping () -> Void
) -> some View {
    ForEach(0..<lane.count, id: \.self) { index in
        let item = lane.itemAt(index)
        Group {
            if let item {
                Button {
                    queue.skipToEntry(item.entryId)
                    onSkipped()
                } label: {
                    QueueRow(item: item)
                }
                .buttonStyle(.plain)
                .swipeActions(edge: .trailing) {
                    Button(role: .destructive) {
                        queue.removeEntry(item.entryId)
                    } label: {
                        Label("Remove", systemImage: "trash")
                    }
                }
            }
            else {
                QueueRowPlaceholder()
            }
        }
        .task(id: QueueRowLoadID(epoch: lane.loadEpoch, index: index)) {
            guard item == nil, let loadRange = lane.loadRange else {
                return
            }
            let first = max(0, index - queueUpcomingLoadBatchSize / 2)
            let end = min(first + queueUpcomingLoadBatchSize, lane.count)
            await loadRange(first, end - first)
        }
    }
    .onMove { source, destination in
        reorderQueueEntry(in: lane, queue: queue, from: source, to: destination)
    }
    .environment(\.editMode, .constant(.active))
}

/// Apply a SwiftUI `onMove` to the queue by entry id. A move whose source or
/// target row isn't loaded is dropped with a warning rather than guessed —
/// reorder only ever targets a rendered (hence loaded) row in practice, so this
/// only ever excludes a brief loading flash, not a real position.
@MainActor
func reorderQueueEntry(
    in lane: QueueLane,
    queue: Queue,
    from source: IndexSet,
    to destination: Int
) {
    guard let from = source.first else {
        // SwiftUI always hands onMove a non-empty source; an empty set is a
        // framework anomaly, so surface it rather than silently dropping it.
        logger.warning("onMove fired with an empty source index set")
        return
    }
    guard let fromItem = lane.itemAt(from) else {
        logger.warning("onMove source at index \(from) is not loaded")
        return
    }
    // SwiftUI's destination is a gap index in the original array (item still
    // present): the moved entry lands before the item currently at that gap,
    // or at the end when the gap is past the last row.
    let beforeEntryId: String?
    if destination < lane.count {
        guard let target = lane.itemAt(destination) else {
            logger.warning("onMove destination at index \(destination) is not loaded")
            return
        }
        beforeEntryId = target.entryId
    }
    else {
        beforeEntryId = nil
    }
    queue.reorderEntry(fromItem.entryId, beforeEntryId)
}

private struct NowPlayingRow: View {
    let track: NowPlayingTrack

    var body: some View {
        HStack(spacing: 12) {
            ImageView(coverImageId: track.coverImageId, pointSize: 44)
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(track.trackTitle)
                    .font(.body)
                    .foregroundStyle(Theme.accent)
                    .lineLimit(1)
                Text(track.artistNames)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
    }
}

/// One "Up Next" row — shared by the queue sheet and the expanded now-playing
/// view's embedded queue.
struct QueueRow: View {
    let item: QueueItem

    var body: some View {
        HStack(spacing: 12) {
            ImageView(coverImageId: item.coverImageId, pointSize: 44)
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.body)
                    .lineLimit(1)
                if !item.albumTitle.isEmpty {
                    Text(item.albumTitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            if !item.durationLabel.isEmpty {
                Text(item.durationLabel)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .padding(.vertical, 4)
    }
}

/// A not-yet-loaded row: a skeleton shape, no text — `loadRange` is already in
/// flight for it via the row's `.task(id:)`.
struct QueueRowPlaceholder: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 4)
                .fill(.secondary.opacity(0.15))
                .frame(width: 44, height: 44)
            VStack(alignment: .leading, spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 160, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 100, height: 10)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
    }
}
