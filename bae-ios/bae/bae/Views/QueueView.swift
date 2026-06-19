import SwiftUI

/// The play queue, presented as a sheet: the currently-playing track, then a
/// reorderable "Up Next" list. Reads the authoritative now-playing and queue
/// off the shared `PlaybackStore`; mutations go straight to the `Queue` service
/// (`clearQueue` / `removeFromQueue` / `reorderQueue` / `skipToQueueIndex`) and
/// reflect back as a `QueueUpdated` event that re-populates `queueItems`.
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
                // Positional identity: the queue may hold the same track twice,
                // so `QueueItem.id` (== trackId) isn't unique. Keying on the
                // enumerated offset makes each row's identity its queue index —
                // which is exactly what onMove/onDelete report back to core.
                ForEach(
                    Array(playbackStore.queueItems.enumerated()),
                    id: \.offset
                ) { index, item in
                    Button {
                        // While editing, a row tap belongs to reorder/delete,
                        // not skip — don't jump tracks and dismiss mid-edit.
                        guard !editMode.isEditing else { return }
                        queue.skipToQueueIndex(UInt32(index))
                        dismiss()
                    } label: {
                        QueueRow(
                            item: item,
                            coverPath: item.coverImageId.flatMap(
                                mediaPaths.imagePathIfExists
                            )
                        )
                    }
                    .buttonStyle(.plain)
                }
                .onMove { source, destination in
                    guard let from = source.first else {
                        return
                    }
                    // SwiftUI's destination is a gap index in the original array
                    // (item still present) — the same convention core's reorder
                    // expects, so map straight through with no offset.
                    queue.reorderQueue(UInt32(from), UInt32(destination))
                }
                .onDelete { offsets in
                    guard let index = offsets.first else {
                        return
                    }
                    queue.removeFromQueue(UInt32(index))
                }
            }
        }
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

private struct QueueRow: View {
    let item: QueueItem
    let coverPath: String?

    var body: some View {
        HStack(spacing: 12) {
            ImageView(path: coverPath, pointSize: 44)
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
