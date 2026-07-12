import BaeKit
import Combine
import SwiftUI

/// Isolates observation so that only slow-changing properties (track info, volume, etc.)
/// trigger SwiftUI re-evaluation. Position ticks bypass SwiftUI entirely via the AppKit
/// SeekBarNSView, which AppService updates directly.
struct NowPlayingBarContainer: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(PlaybackStore.self)
    var playbackStore
    @Environment(UiStore.self)
    var uiStore
    let onDropToQueue: ([String]) -> Void

    var body: some View {
        @Bindable
        var uiStore = uiStore
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.coverImageId
            .map {
                .library(.cover(id: $0, version: nil))
            }
        NowPlayingBar(
            trackTitle: track?.trackTitle,
            secondaryLine: np.secondaryLine,
            cover: cover,
            isPlaying: np.isPlaying,
            isLoading: np.loadingTrackId != nil,
            durationMs: track?.durationMs,
            volume: playbackStore.volume,
            isMuted: playbackStore.isMuted,
            repeatMode: playbackStore.repeatMode,
            showQueue: $uiStore.showQueue,
            onPlayPause: { playback.playPause(for: playbackStore.nowPlaying) },
            onNext: { playback.nextTrack() },
            onPrevious: { playback.previousTrack() },
            onSeek: { ratio in
                playbackStore.projectSeek(ratio: ratio)
                playback.seekByRatio(ratio)
            },
            onVolumeChange: { playback.setVolume($0) },
            onToggleMute: { playback.setMuted(!playbackStore.isMuted) },
            onCycleRepeat: {
                playback.setRepeatMode(playbackStore.repeatMode.next)
            },
            onDropToQueue: onDropToQueue,
            onNavigateToAlbum: {
                if let albumId = track?.albumId {
                    uiStore.navigateToAlbum(albumId)
                }
            },
            queueAddPublisher: playbackStore.queueItemsAddedSubject
                .eraseToAnyPublisher(),
        )
        .sidePausePromptAlert()
    }
}

struct NowPlayingBar: View {
    let trackTitle: String?
    let secondaryLine: String?
    let cover: ImageContent?
    let isPlaying: Bool
    let isLoading: Bool
    let durationMs: UInt64?
    let volume: Float
    let isMuted: Bool
    let repeatMode: BridgeRepeatMode
    @Binding
    var showQueue: Bool
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSeek: (Double) -> Void
    let onVolumeChange: (Float) -> Void
    let onToggleMute: () -> Void
    let onCycleRepeat: () -> Void
    let onDropToQueue: ([String]) -> Void
    let onNavigateToAlbum: () -> Void
    let queueAddPublisher: AnyPublisher<Int, Never>

    @State
    private var queueButtonDropTargeted = false

    var body: some View {
        HStack(spacing: 16) {
            trackInfo
                .frame(width: 220, alignment: .leading)

            Spacer()

            transportControls

            Spacer()

            trailingControls
                .frame(width: 180, alignment: .trailing)
        }
        .padding(.horizontal, 16)
        .frame(height: 72)
        .background(Theme.surface)
    }

    // MARK: - Left: track info

    private var trackInfo: some View {
        HStack(spacing: 12) {
            if trackTitle != nil {
                Button(action: onNavigateToAlbum) {
                    albumArt
                        .frame(width: 48, height: 48)
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .accessibilityLabel("Album art")
                }
                .buttonStyle(.plain)
            }

            VStack(alignment: .leading, spacing: 2) {
                if let title = trackTitle {
                    Button(action: onNavigateToAlbum) {
                        Text(title)
                            .font(.callout)
                            .fontWeight(.medium)
                            .lineLimit(1)
                    }
                    .buttonStyle(.plain)
                }

                if let secondaryLine {
                    Text(secondaryLine)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
    }

    private var albumArt: some View {
        ImageView(content: cover, pointSize: 48)
    }

    // MARK: - Center: transport controls + progress

    private var transportControls: some View {
        VStack(spacing: 4) {
            HStack(spacing: 20) {
                Button(action: onPrevious) {
                    Image(systemName: "backward.fill")
                        .font(.title3)
                        .frame(width: 32, height: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Previous track")
                .accessibilityLabel("Previous track")

                PlayPauseControl(
                    isPlaying: isPlaying,
                    isLoading: isLoading,
                    glyphFont: .title2,
                    spinnerControlSize: .small,
                    targetSize: 36,
                    onToggle: onPlayPause
                )

                Button(action: onNext) {
                    Image(systemName: "forward.fill")
                        .font(.title3)
                        .frame(width: 32, height: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Next track")
                .accessibilityLabel("Next track")
            }

            progressBar
        }
    }

    private var progressBar: some View {
        PlaybackProgressRepresentable(
            durationMs: durationMs,
            onSeek: onSeek,
        )
        .frame(width: 396, height: 20)
        .accessibilityLabel("Playback position")
    }

    // MARK: - Right: volume + repeat

    private var trailingControls: some View {
        HStack(spacing: 12) {
            repeatButton

            Button(action: { showQueue.toggle() }) {
                Image(systemName: "list.bullet")
                    .foregroundColor(showQueue ? .accentColor : .secondary)
                    .frame(width: 30, height: 30)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .font(.title3)
            .help("Queue")
            .accessibilityLabel("Queue")
            .padding(4)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(
                        queueButtonDropTargeted
                            ? Color.accentColor.opacity(0.3) : Color.clear
                    ),
            )
            .scaleEffect(queueButtonDropTargeted ? 1.15 : 1.0)
            .animation(
                .easeInOut(duration: 0.15),
                value: queueButtonDropTargeted
            )
            .dropDestination(for: String.self) { droppedIds, _ in
                queueButtonDropTargeted = false
                guard !droppedIds.isEmpty else {
                    return false
                }
                onDropToQueue(droppedIds)
                return true
            } isTargeted: { targeted in
                queueButtonDropTargeted = targeted
            }
            .overlay(alignment: .topTrailing) {
                QueueAddBadge(
                    events: queueAddPublisher,
                    scheduler: .main,
                    style: QueueAddBadgeStyle(
                        textFont: .system(size: 10, weight: .semibold),
                        symbolFont: .system(size: 8.5, weight: .bold),
                        padding: EdgeInsets(
                            top: 1,
                            leading: 5,
                            bottom: 1,
                            trailing: 5
                        ),
                        fill: .accentColor,
                        offset: CGSize(width: 6, height: -6)
                    )
                )
            }

            Button(action: onToggleMute) {
                Image(
                    systemName: isMuted ? "speaker.slash.fill" : "speaker.fill"
                )
                .foregroundStyle(.secondary)
                .frame(width: 30, height: 30)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .font(.title3)
            .help(isMuted ? "Unmute" : "Mute")
            .accessibilityLabel(isMuted ? "Unmute" : "Mute")

            Slider(
                value: Binding(
                    get: { volume },
                    set: { onVolumeChange($0) },
                ),
                in: 0...1,
            )
            .frame(width: 80)
            .accessibilityLabel("Volume")
        }
    }

    private var repeatButton: some View {
        Button(action: onCycleRepeat) {
            Group {
                switch repeatMode {
                case .off:
                    Image(systemName: "repeat")
                        .foregroundStyle(.secondary)
                case .context:
                    Image(systemName: "repeat")
                        .foregroundColor(.accentColor)
                case .track:
                    Image(systemName: "repeat.1")
                        .foregroundColor(.accentColor)
                }
            }
            .frame(width: 30, height: 30)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .font(.title3)
        .help(repeatHelp)
        .accessibilityLabel(repeatHelp)
    }

    private var repeatHelp: LocalizedStringKey {
        switch repeatMode {
        case .off:
            "Repeat: off"
        case .context:
            "Repeat: context"
        case .track:
            "Repeat: track"
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Preview host that provides the state bindings NowPlayingBar needs.
    /// The playback position publisher defaults to `Empty()` — the NSView
    /// renders the duration clock from the static `durationMs` prop.
    private struct NowPlayingBarPreview: View {
        let trackTitle: String?
        let artistNames: String?
        let isPlaying: Bool
        var isLoading: Bool = false
        let repeatMode: BridgeRepeatMode

        @State
        private var showQueue = false
        @State
        private var volume: Float = 0.7
        @State
        private var isMuted = false

        var body: some View {
            NowPlayingBar(
                trackTitle: trackTitle,
                secondaryLine: artistNames,
                cover: nil,
                isPlaying: isPlaying,
                isLoading: isLoading,
                durationMs: 222_000,
                volume: volume,
                isMuted: isMuted,
                repeatMode: repeatMode,
                showQueue: $showQueue,
                onPlayPause: {},
                onNext: {},
                onPrevious: {},
                onSeek: { _ in },
                onVolumeChange: { volume = $0 },
                onToggleMute: { isMuted.toggle() },
                onCycleRepeat: {},
                onDropToQueue: { _ in },
                onNavigateToAlbum: {},
                queueAddPublisher: Empty().eraseToAnyPublisher(),
            )
            .frame(width: 1100)
        }
    }

    #Preview("Playing") {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: true,
            repeatMode: .off,
        )
        .environment(MediaPaths.stub)
        .environment(PreviewData.queueStore(manualCount: 2))
        .environment(Queue.stub)
    }

    #Preview("Paused — Repeat Context") {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: false,
            repeatMode: .context,
        )
        .environment(MediaPaths.stub)
        .environment(PreviewData.queueStore(manualCount: 2, shuffled: true))
        .environment(Queue.stub)
    }

    #Preview("Loading") {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: true,
            isLoading: true,
            repeatMode: .off,
        )
        .environment(MediaPaths.stub)
        .environment(PreviewData.queueStore(manualCount: 5, context: nil))
        .environment(Queue.stub)
    }

    #Preview("Empty") {
        NowPlayingBarPreview(
            trackTitle: nil,
            artistNames: nil,
            isPlaying: false,
            repeatMode: .off,
        )
        .environment(MediaPaths.stub)
        .environment(PreviewData.queueStore(manualCount: 0, context: nil))
        .environment(Queue.stub)
    }
#endif
