import SwiftUI

/// Full-screen now-playing player, presented as a `.fullScreenCover` from the
/// compact `NowPlayingBar` when its cover / title area is tapped. Reads the same
/// single source as the bar — the shared `PlaybackStore`, driven by core's
/// `BridgeUiEvent`s — and sends transport / seek / repeat / volume / mute
/// through `Playback` (all non-throwing fire-and-forget).
///
/// Pure iterate-and-render: the seek labels arrive pre-formatted on the position
/// subject; this view formats nothing.
struct ExpandedNowPlayingView: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback
    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(\.dismiss)
    private var dismiss

    @State
    private var showQueue = false
    /// While the user drags the volume slider, follow the finger from this local
    /// value; the round-tripped `playbackStore.volume` would otherwise snap the
    /// thumb back mid-drag. `nil` means "not dragging".
    @State
    private var dragVolume: Float?

    var body: some View {
        // When the track clears (e.g. playback stops) the cover collapses back
        // to the bar; there's nothing to show full-screen.
        if let track = playbackStore.nowPlaying.track {
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

                ImageView(
                    path: track.coverImageId
                        .flatMap(mediaPaths.imagePathIfExists),
                    pointSize: 320
                )
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
                    onSeek: { playback.seekByRatio($0) }
                )

                transport

                controls

                volume
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 16)
            .frame(maxHeight: .infinity, alignment: .top)
            .background(Theme.background)
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            .sheet(isPresented: $showQueue) {
                QueueView()
            }
        }
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

    private var controls: some View {
        HStack(spacing: 48) {
            Button {
                playback.cycleRepeatMode()
            } label: {
                // Dimmed when off; accented when on (repeat-one glyph for track).
                Image(
                    systemName: playbackStore.repeatMode == .track
                        ? "repeat.1" : "repeat"
                )
                .foregroundStyle(
                    playbackStore.repeatMode == .none
                        ? Color.secondary : Theme.accent
                )
            }
            .accessibilityLabel("Repeat mode")
            Button {
                showQueue = true
            } label: {
                Image(systemName: "list.bullet")
            }
            .accessibilityLabel("Queue")
        }
    }

    private var volume: some View {
        HStack(spacing: 12) {
            Button {
                playback.toggleMute()
            } label: {
                Image(
                    systemName: playbackStore.isMuted
                        ? "speaker.slash.fill" : "speaker.fill"
                )
                .foregroundStyle(.secondary)
            }
            .accessibilityLabel(playbackStore.isMuted ? "Unmute" : "Mute")
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
}
