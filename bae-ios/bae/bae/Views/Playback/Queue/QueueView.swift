import BaeKit
import SwiftUI

/// The play queue sheet: the playing track, then the Up Next and context
/// lanes; edits go to `Queue` and come back through `PlaybackStore`.
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
                // Each lane's Clear sits in its own section header.
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    @ViewBuilder
    private var upNext: some View {
        if playbackStore.manualQueue.isEmpty {
            // A context section below stands in for the empty message.
            if playbackStore.queueContext == nil {
                Section {
                    Text(
                        playbackStore.nowPlaying.track == nil
                            ? String(localized: "Queue is empty")
                            : String(localized: "Nothing up next")
                    )
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, ThemeSpace.section)
                }
            }
        }
        else {
            Section {
                // The manual lane is fully loaded, so it has no load hook.
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

    // The not-yet-played rest of the context, loaded in ranges as rows appear.
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

/// The Up Next header, shared with the expanded player's embedded queue.
@MainActor
@ViewBuilder
func upNextHeader(queue: Queue) -> some View {
    HStack(spacing: ThemeSpace.compact) {
        Text("Up Next")
        Spacer()
        clearLaneButton(label: Text("Clear Up Next")) { queue.clearUpNext() }
    }
}

/// The context section header with its Clear and shuffle toggle, shared with
/// the expanded player's embedded queue.
@MainActor
@ViewBuilder
func playingFromHeader(
    kind: BridgePlaybackSourceKind,
    shuffled: Bool,
    queue: Queue
) -> some View {
    HStack(spacing: ThemeSpace.compact) {
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

/// A lane's Clear button; `label` names the lane for VoiceOver.
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

/// The context section's title for what it plays from.
func contextSectionTitle(_ kind: BridgePlaybackSourceKind) -> LocalizedStringKey {
    switch kind {
    case .release:
        return "Playing From"
    case .library:
        return "Your Library"
    }
}

private struct NowPlayingRow: View {
    let track: BridgeNowPlayingTrack

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: track.display.coverImage, pointSize: ThemeSize.rowArtwork)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(track.display.title)
                    .themeText(.rowTitle)
                    .foregroundStyle(Theme.accent)
                    .lineLimit(1)
                Text(track.display.artistNames)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
    }
}

#if DEBUG
#Preview {
    QueueView()
        .previewStores()
}
#endif
