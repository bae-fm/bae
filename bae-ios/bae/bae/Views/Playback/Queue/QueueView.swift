import BaeKit
import SwiftUI

/// The play queue, presented as a sheet: the currently-playing track, then a
/// reorderable "Up Next" list. Reads the authoritative now-playing and queue
/// off the shared `PlaybackStore`; mutations go straight to the `Queue` service
/// (`clearUpNext` / `clearPlayingFrom` / `removeEntry` / `reorderEntry` /
/// `skipToEntry`) and reflect back as a `QueueUpdated` event that re-populates
/// `queueItems`.
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
                // Each lane clears itself from its own section header, so the
                // toolbar carries only Done.
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
            Section {
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
            } header: {
                upNextHeader(queue: queue)
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

/// The manual lane's header — shared by the queue sheet and the expanded
/// player's embedded queue so the control can't drift. Both surfaces render this
/// section only while the lane has rows, which is exactly when its Clear should
/// be present.
@MainActor
@ViewBuilder
func upNextHeader(queue: Queue) -> some View {
    HStack(spacing: 6) {
        Text("Up Next")
        Spacer()
        clearLaneButton(label: Text("Clear Up Next")) { queue.clearUpNext() }
    }
}

/// The context-section header with its Clear and shuffle toggle — shared by the
/// queue sheet and the expanded player's embedded queue so the controls can't
/// drift. The title names what's playing — a release ("Playing From") vs the
/// whole library — by `kind`. The toggle is tinted when on; tapping it flips the
/// context's order while the current track keeps playing. Clearing drops the
/// whole section; the playing track keeps playing.
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
        clearLaneButton(label: Text("Clear Playing From")) {
            queue.clearPlayingFrom()
        }
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

/// A lane's Clear, as it sits in that lane's section header. The visible word
/// stays "Clear" — the header beside it already names the lane — while `label`
/// spells the lane out for VoiceOver, which reads the button on its own.
@MainActor
@ViewBuilder
func clearLaneButton(
    label: Text,
    action: @escaping () -> Void
) -> some View {
    Button("Clear", action: action)
        .buttonStyle(.plain)
        .foregroundStyle(Theme.accent)
        .accessibilityLabel(label)
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
