import BaeKit
import SwiftUI

/// Persistent now-playing bar, hidden until a track is loaded.
struct NowPlayingBar: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback
    @Environment(ConfigStore.self)
    private var configStore

    @State
    private var showQueue = false
    @State
    private var showExpanded = false

    var body: some View {
        if let track = playbackStore.nowPlaying.track {
            VStack(spacing: ThemeSpace.compact) {
                transport(track: track)
                ProgressBar(
                    positionPublisher: playbackStore.playbackPositionPublisher,
                    showRemainingTime: configStore.config.showRemainingTime,
                    onSeek: { ratio in
                        playbackStore.projectSeek(ratio: ratio)
                        playback.seekByRatio(ratio)
                    },
                    onToggleRemainingTime: {
                        // The config subscription re-renders the bar; nothing
                        // flips locally.
                        let showRemaining = !configStore.config.showRemainingTime
                        Task {
                            do {
                                try await playback.setShowRemainingTime(
                                    showRemaining
                                )
                            }
                            catch {
                                configStore.showError(error)
                            }
                        }
                    }
                )
            }
            .padding(.horizontal, ThemeSpace.group)
            .padding(.vertical, ThemeSpace.related)
            .background(Theme.surface)
            .sheet(isPresented: $showQueue) {
                QueueView()
            }
            // A sheet, not a full-screen cover, so a swipe down closes it.
            .sheet(isPresented: $showExpanded) {
                ExpandedNowPlayingView()
                    .presentationDetents([.large])
                    .presentationDragIndicator(.visible)
            }
            .sidePausePromptAlert(showError: { configStore.showError($0) })
        }
    }

    private func transport(track: NowPlayingTrack) -> some View {
        HStack(spacing: ThemeSpace.group) {
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
            HStack(spacing: ThemeSpace.group) {
                ImageView(imageRef: track.coverImage, pointSize: ThemeSize.barArtwork)
                    .frame(
                        width: ThemeSize.barArtwork,
                        height: ThemeSize.barArtwork
                    )
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text(track.trackTitle)
                        .themeText(.rowTitle)
                        .lineLimit(1)
                    if let secondaryLine {
                        Text(secondaryLine)
                            .themeText(.detail)
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
            glyphFont: ThemeIcon.large.font,
            spinnerControlSize: .regular,
            onToggle: { playback.playPause(for: playbackStore.nowPlaying) }
        )
        Button {
            playback.nextTrack()
        } label: {
            Image(systemName: "forward.fill")
        }
        .accessibilityLabel("Next track")
        CastButton()
        Button {
            showQueue = true
        } label: {
            Image(systemName: "list.bullet")
        }
        .accessibilityLabel("Queue")
        .overlay(alignment: .topTrailing) {
            QueueAddBadge(
                events: playbackStore.queueItemsAddedPublisher,
                scheduler: .main,
                style: QueueAddBadgeStyle(
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

#if DEBUG
#Preview {
    NowPlayingBar()
        .previewStores()
}
#endif
