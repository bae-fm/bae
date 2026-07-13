import BaeKit
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
                    onSeek: { ratio in
                        playbackStore.projectSeek(ratio: ratio)
                        playback.seekByRatio(ratio)
                    }
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
            .sidePausePromptAlert()
        }
    }

    private func transport(track: NowPlayingTrack) -> some View {
        HStack(spacing: 12) {
            trackInfoButton(
                track: track,
                secondaryLine: playbackStore.nowPlaying.secondaryLine
            )
            transportButtons
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
    }

    // Cover + title/artist expand into the full-screen player; the transport
    // buttons stay outside this tap target.
    private func trackInfoButton(
        track: NowPlayingTrack,
        secondaryLine: String?
    ) -> some View {
        Button {
            showExpanded = true
        } label: {
            HStack(spacing: 12) {
                ImageView(coverImageId: track.coverImageId, pointSize: 48)
                    .frame(width: 48, height: 48)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                VStack(alignment: .leading, spacing: 2) {
                    Text(track.trackTitle)
                        .font(.subheadline.weight(.medium))
                        .lineLimit(1)
                    if let secondaryLine {
                        Text(secondaryLine)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
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
            onToggle: { playback.playPause(for: playbackStore.nowPlaying) }
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
        .overlay(alignment: .topTrailing) {
            QueueAddBadge(
                events: playbackStore.queueItemsAddedSubject
                    .eraseToAnyPublisher(),
                scheduler: .main,
                style: QueueAddBadgeStyle(
                    textFont: .system(size: 11, weight: .semibold),
                    symbolFont: .system(size: 9, weight: .bold),
                    padding: EdgeInsets(
                        top: 3,
                        leading: 7,
                        bottom: 3,
                        trailing: 7
                    ),
                    fill: Theme.accent,
                    offset: CGSize(width: 10, height: -10)
                )
            )
        }
        Button {
            playback.setRepeatMode(
                bridgeNextRepeatMode(mode: playbackStore.repeatMode)
            )
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
}
