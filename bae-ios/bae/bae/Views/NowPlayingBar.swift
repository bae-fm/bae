import SwiftUI

/// Persistent now-playing bar. Reads transport state from the shared
/// `PlaybackStore` and the high-frequency position from its Combine subject,
/// and sends transport commands through `Playback`. Hidden until a track is
/// loaded. Tapping the cover / title area expands into the full-screen
/// `ExpandedNowPlayingView`.
struct NowPlayingBar: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback
    @Environment(MediaPaths.self)
    private var mediaPaths

    @State
    private var showQueue = false
    @State
    private var showExpanded = false

    var body: some View {
        if let track = playbackStore.nowPlaying.track {
            VStack(spacing: 6) {
                transport(track: track)
                ProgressBar(
                    positionSubject: playbackStore.playbackPositionSubject,
                    onSeek: { playback.seekByRatio($0) }
                )
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Theme.surface)
            .sheet(isPresented: $showQueue) {
                QueueView()
            }
            // A sheet (not a cover) so the player can be swiped down to dismiss
            // and its embedded queue swiped up to scroll — the sheet and the
            // list coordinate that handoff.
            .sheet(isPresented: $showExpanded) {
                ExpandedNowPlayingView()
                    .presentationDetents([.large])
                    .presentationDragIndicator(.visible)
            }
        }
    }

    private func transport(track: NowPlayingTrack) -> some View {
        HStack(spacing: 12) {
            trackInfoButton(track: track)
            transportButtons
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
    }

    // Cover + title/artist expand into the full-screen player; the transport
    // buttons stay outside this tap target.
    private func trackInfoButton(track: NowPlayingTrack) -> some View {
        Button {
            showExpanded = true
        } label: {
            HStack(spacing: 12) {
                ImageView(
                    path: track.coverImageId
                        .flatMap(mediaPaths.imagePathIfExists),
                    pointSize: 48
                )
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                VStack(alignment: .leading, spacing: 2) {
                    Text(track.trackTitle)
                        .font(.subheadline.weight(.medium))
                        .lineLimit(1)
                    Text(track.artistNames)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .accessibilityLabel("Expand now playing")
    }

    @ViewBuilder
    private var transportButtons: some View {
        Button {
            playback.previousTrack()
        } label: {
            Image(systemName: "backward.fill")
        }
        .accessibilityLabel("Previous track")
        PlayPauseControl(
            isPlaying: playbackStore.nowPlaying.isPlaying,
            isLoading: playbackStore.nowPlaying.loadingTrackId != nil,
            glyphFont: .title3,
            spinnerControlSize: .regular,
            onToggle: { playback.togglePlayPause() }
        )
        Button {
            playback.nextTrack()
        } label: {
            Image(systemName: "forward.fill")
        }
        .accessibilityLabel("Next track")
        Button {
            showQueue = true
        } label: {
            Image(systemName: "list.bullet")
        }
        .accessibilityLabel("Queue")
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
    }
}
