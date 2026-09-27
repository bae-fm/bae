import BaeKit
import SwiftUI

/// Full-screen player in a sheet opened from `NowPlayingBar`, with the upcoming
/// queue below it in the same scroll.
struct ExpandedNowPlayingView: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(\.dismiss)
    private var dismiss

    /// The volume while dragging, so incoming volume updates don't snap the
    /// thumb back; `nil` when not dragging.
    @State
    private var dragVolume: Float?

    var body: some View {
        // Without a track the bar hides and takes this sheet with it.
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

            ImageView(imageRef: track.coverImage, pointSize: 320)
                .aspectRatio(1, contentMode: .fit)
                .frame(maxWidth: .infinity)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))

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
                positionPublisher: playbackStore.playbackPositionPublisher,
                showRemainingTime: configStore.config.showRemainingTime,
                onSeek: { ratio in
                    playbackStore.projectSeek(ratio: ratio)
                    playback.seekByRatio(ratio)
                },
                onToggleRemainingTime: {
                    // The config subscription re-renders the bar; nothing flips
                    // locally.
                    let showRemaining = !configStore.config.showRemainingTime
                    Task {
                        do {
                            try await playback.setShowRemainingTime(showRemaining)
                        }
                        catch {
                            configStore.showError(error)
                        }
                    }
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
                onToggle: { playback.playPause(for: playbackStore.nowPlaying) }
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

    private var repeatControl: some View {
        Button {
            playback.setRepeatMode(
                bridgeNextRepeatMode(mode: playbackStore.repeatMode)
            )
        } label: {
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

    // The upcoming queue; the current track is the player above, and tapping a
    // row skips to it without closing the sheet.
    @ViewBuilder
    private var upNext: some View {
        if !playbackStore.manualQueue.isEmpty {
            Section {
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
            } header: {
                upNextHeader(queue: queue)
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

#if DEBUG
#Preview {
    ExpandedNowPlayingView()
        .previewStores()
}
#endif
