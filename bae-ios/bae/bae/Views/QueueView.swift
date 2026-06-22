import SwiftUI
import os.log

private let logger = Logger.bae("Queue")

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
    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(\.dismiss)
    private var dismiss
    // Own the edit state rather than letting EditButton drive an implicit one,
    // so it can be forced inactive when the queue empties (otherwise deleting
    // the last row mid-edit would strand the list in edit mode) and so row taps
    // can be gated while editing.
    @State
    private var editMode: EditMode = .inactive

    var body: some View {
        NavigationStack {
            List {
                if let track = playbackStore.nowPlaying.track {
                    Section("Now Playing") {
                        NowPlayingRow(
                            track: track,
                            coverPath: track.coverImageId.flatMap(
                                mediaPaths.imagePathIfExists
                            )
                        )
                    }
                }

                upNext
            }
            .listStyle(.plain)
            .environment(\.editMode, $editMode)
            .onChange(of: playbackStore.queueItems.isEmpty) { _, isEmpty in
                if isEmpty { editMode = .inactive }
            }
            .background(Theme.background)
            .navigationTitle("Queue")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Clear") { queue.clearQueue() }
                        .disabled(playbackStore.queueItems.isEmpty)
                }
                // EditButton toggles the list into edit mode, which is what
                // surfaces the drag-to-reorder handles (and the delete control)
                // for the Up Next rows; without it `.onMove` has no affordance.
                ToolbarItem(placement: .topBarTrailing) {
                    EditButton()
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    @ViewBuilder
    private var upNext: some View {
        if playbackStore.queueItems.isEmpty {
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
        else {
            Section("Up Next") {
                // Edit mode surfaces reorder handles here; a row tap is gated to
                // skip only when not editing, and skipping dismisses the sheet.
                upNextRows(
                    items: playbackStore.queueItems,
                    queue: queue,
                    isEditing: editMode.isEditing,
                    onSkipped: { dismiss() }
                )
            }
        }
    }
}

/// The "Up Next" rows — shared by the queue sheet and the expanded player's
/// embedded queue so the index conventions can't drift. Tapping a row skips to
/// it (unless `isEditing`, when the tap belongs to reorder/delete) and then runs
/// `onSkipped`; swiping deletes; `.onMove` reorders. `.onMove` is inert without
/// an active edit mode, so the embedded queue (which has none) shows no reorder
/// handles while still getting tap-to-skip and swipe-to-delete.
@MainActor
@ViewBuilder
func upNextRows(
    items: [QueueItem],
    queue: Queue,
    isEditing: Bool,
    onSkipped: @escaping () -> Void
) -> some View {
    // Each item carries a unique per-instance `entryId`, so rows key on
    // `QueueItem.id` (== entryId) even when the same track is queued twice.
    // onMove/onDelete report array positions, which we resolve back to the entry
    // id of the affected row.
    ForEach(items) { item in
        Button {
            guard !isEditing else { return }
            queue.skipToEntry(item.entryId)
            onSkipped()
        } label: {
            QueueRow(item: item)
        }
        .buttonStyle(.plain)
    }
    .onMove { source, destination in
        guard let from = source.first else {
            // SwiftUI always hands onMove a non-empty source; an empty set is a
            // framework anomaly, so surface it rather than silently dropping it.
            logger.warning("onMove fired with an empty source index set")
            return
        }
        // SwiftUI's destination is a gap index in the original array (item still
        // present): the moved entry lands before the item currently at that gap,
        // or at the end when the gap is past the last row.
        let beforeEntryId =
            destination < items.count ? items[destination].entryId : nil
        queue.reorderEntry(items[from].entryId, beforeEntryId)
    }
    .onDelete { offsets in
        guard let index = offsets.first else {
            logger.warning("onDelete fired with an empty offset set")
            return
        }
        queue.removeEntry(items[index].entryId)
    }
}

private struct NowPlayingRow: View {
    let track: NowPlayingTrack
    let coverPath: String?

    var body: some View {
        HStack(spacing: 12) {
            ImageView(path: coverPath, pointSize: 44)
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
/// view's embedded queue. Resolves its own cover path so the lookup (and its
/// dependency on `MediaPaths`) stays at the leaf.
struct QueueRow: View {
    @Environment(MediaPaths.self)
    private var mediaPaths
    let item: QueueItem

    var body: some View {
        HStack(spacing: 12) {
            ImageView(
                path: item.coverImageId.flatMap(mediaPaths.imagePathIfExists),
                pointSize: 44
            )
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
