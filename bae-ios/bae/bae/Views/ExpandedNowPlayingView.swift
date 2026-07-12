import BaeKit
import SwiftUI

/// Full-screen now-playing player, presented as a `.sheet` (swipe down to
/// dismiss) from the compact `NowPlayingBar` when its cover / title area is
/// tapped. The player fills the first screen and the upcoming queue sits below it
/// in the same scroll, so a swipe up scrolls into the queue and a swipe down from
/// the top dismisses (the sheet and the list coordinate that handoff). Reads the
/// shared `PlaybackStore` — driven by core's `BridgeUiEvent`s — and sends
/// transport / seek / repeat / volume / mute through `Playback` and queue
/// mutations through `Queue` (all non-throwing fire-and-forget).
///
/// Pure iterate-and-render: the seek labels arrive pre-formatted on the position
/// subject; this view formats nothing.
struct ExpandedNowPlayingView: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue
    @Environment(\.dismiss)
    private var dismiss

    /// While the user drags the volume slider, follow the finger from this local
    /// value; the round-tripped `playbackStore.volume` would otherwise snap the
    /// thumb back mid-drag. `nil` means "not dragging".
    @State
    private var dragVolume: Float?

    var body: some View {
        // When the track clears (e.g. playback stops) the bar collapses and takes
        // this sheet down with it; there's nothing to show.
        if let track = playbackStore.nowPlaying.track {
            List {
                Section {
                    player(track: track)
                        .listRowInsets(EdgeInsets())
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                }
                upNext
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
        }
    }

    private func player(track: NowPlayingTrack) -> some View {
        VStack(spacing: 24) {
            HStack {
                Button {
                    dismiss()
                } label: {
                    Image(systemName: "chevron.down")
                        .font(.title3)
                }
                .accessibilityLabel("Collapse")
                Spacer(minLength: 0)
            }

            ImageView(coverImageId: track.coverImageId, pointSize: 320)
                .aspectRatio(1, contentMode: .fit)
                .frame(maxWidth: .infinity)
                .clipShape(RoundedRectangle(cornerRadius: 8))

            VStack(alignment: .leading, spacing: 4) {
                Text(track.trackTitle)
                    .font(.title2.weight(.bold))
                    .lineLimit(1)
                Text(track.artistNames)
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            ProgressBar(
                positionSubject: playbackStore.playbackPositionSubject,
                onSeek: { ratio in
                    playbackStore.projectSeek(ratio: ratio)
                    playback.seekByRatio(ratio)
                }
            )

            transport

            repeatControl

            volume
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 16)
    }

    private var transport: some View {
        HStack(spacing: 48) {
            Button {
                playback.previousTrack()
            } label: {
                Image(systemName: "backward.fill")
                    .font(.title)
            }
            .accessibilityLabel("Previous track")
            PlayPauseControl(
                isPlaying: playbackStore.nowPlaying.isPlaying,
                isLoading: playbackStore.nowPlaying.loadingTrackId != nil,
                glyphFont: .largeTitle,
                spinnerControlSize: .large,
                onToggle: { playback.togglePlayPause() }
            )
            Button {
                playback.nextTrack()
            } label: {
                Image(systemName: "forward.fill")
                    .font(.title)
            }
            .accessibilityLabel("Next track")
        }
    }

    // Repeat-mode toggle. The queue is embedded below the player now, so there's
    // no separate button to open it as a sheet.
    private var repeatControl: some View {
        Button {
            playback.cycleRepeatMode()
        } label: {
            // Dimmed when off; accented when on (repeat-one glyph for track).
            Image(
                systemName: playbackStore.repeatMode == .track
                    ? "repeat.1" : "repeat"
            )
            .foregroundStyle(
                playbackStore.repeatMode == .off
                    ? Color.secondary : Theme.accent
            )
        }
        .accessibilityLabel("Repeat mode")
    }

    private var volume: some View {
        HStack(spacing: 12) {
            Button {
                playback.setMuted(!playbackStore.isMuted)
            } label: {
                Image(
                    systemName: playbackStore.isMuted
                        ? "speaker.slash.fill" : "speaker.fill"
                )
                .foregroundStyle(.secondary)
            }
            .accessibilityLabel(
                playbackStore.isMuted
                    ? String(localized: "Unmute") : String(localized: "Mute")
            )
            Slider(
                value: Binding(
                    get: { dragVolume ?? playbackStore.volume },
                    set: {
                        dragVolume = $0
                        playback.setVolume($0)
                    }
                ),
                in: 0...1,
                onEditingChanged: { editing in
                    if !editing { dragVolume = nil }
                }
            )
            .accessibilityLabel("Volume")
        }
    }

    // The upcoming queue, embedded below the player so a swipe up scrolls into it.
    // Tapping a row skips to it (without dismissing — the player follows the new
    // track); swiping a row removes it; drag-to-reorder is always available too
    // (`upNextRows` scopes edit mode to its own rows). Clear stays in the
    // dedicated queue sheet (the bar's queue button) — this embedded surface has
    // no toolbar to put it in. The current track isn't in `queueItems` — it's the
    // player above. The section is dropped when empty so no bare "Up Next" header
    // shows.
    @ViewBuilder
    private var upNext: some View {
        if !playbackStore.manualQueue.isEmpty {
            Section("Up Next") {
                let manual = playbackStore.manualQueue
                upNextRows(
                    lane: QueueLane(
                        count: manual.count,
                        itemAt: { manual.indices.contains($0) ? manual[$0] : nil },
                        loadEpoch: 0,
                        loadRange: nil
                    ),
                    queue: queue,
                    onSkipped: {}
                )
            }
        }
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
                    onSkipped: {}
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
