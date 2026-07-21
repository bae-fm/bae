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
    @Environment(ConfigStore.self)
    var configStore
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
            showRemainingTime: configStore.config.showRemainingTime,
            volume: playbackStore.volume,
            isMuted: playbackStore.isMuted,
            repeatMode: playbackStore.repeatMode,
            // nil while no playing context — the bar disables shuffle then.
            shuffled: playbackStore.queueContext?.shuffled,
            showQueue: $uiStore.showQueue,
            onPlayPause: { playback.playPause(for: playbackStore.nowPlaying) },
            onNext: { playback.nextTrack() },
            onPrevious: { playback.previousTrack() },
            onSeek: { ratio in
                playbackStore.projectSeek(ratio: ratio)
                playback.seekByRatio(ratio)
            },
            onToggleRemainingTime: {
                // Write-through: the config invalidation is what re-renders the
                // bar, so there is nothing to flip locally.
                try? playback.setShowRemainingTime(
                    !configStore.config.showRemainingTime
                )
            },
            onVolumeChange: { playback.setVolume($0) },
            onToggleMute: { playback.setMuted(!playbackStore.isMuted) },
            onSetShuffle: { queue.setShuffle($0) },
            onCycleRepeat: {
                playback.setRepeatMode(
                    bridgeNextRepeatMode(mode: playbackStore.repeatMode)
                )
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
    /// The user's elapsed-vs-remaining choice, from the config.
    let showRemainingTime: Bool
    let volume: Float
    let isMuted: Bool
    let repeatMode: BridgeRepeatMode
    /// Playing-context shuffle state: `true`/`false` when a context is playing,
    /// `nil` when there is none — which disables the shuffle button.
    let shuffled: Bool?
    @Binding
    var showQueue: Bool
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void
    let onVolumeChange: (Float) -> Void
    let onToggleMute: () -> Void
    let onSetShuffle: (Bool) -> Void
    let onCycleRepeat: () -> Void
    let onDropToQueue: ([String]) -> Void
    let onNavigateToAlbum: () -> Void
    let queueAddPublisher: AnyPublisher<Int, Never>

    @State
    private var queueButtonDropTargeted = false
    @State
    private var playHovering = false

    var body: some View {
        HStack(spacing: 0) {
            trackInfo
                .frame(maxWidth: .infinity, alignment: .leading)

            centerColumn
                .frame(width: 460)

            trailingControls
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding(.vertical, 14)
        .padding(.horizontal, 22)
        .background(cardChrome)
    }

    // MARK: - Card chrome

    private var cardChrome: some View {
        RoundedRectangle(cornerRadius: 16)
            .fill(.ultraThinMaterial)
            .overlay(
                RoundedRectangle(cornerRadius: 16)
                    .fill(
                        LinearGradient(
                            colors: [Theme.surfaceElevated, Theme.surface],
                            startPoint: .top,
                            endPoint: .bottom,
                        ),
                    )
                    .opacity(0.92),
            )
            .overlay(
                RoundedRectangle(cornerRadius: 16)
                    .strokeBorder(Color.white.opacity(0.08), lineWidth: 1),
            )
            .shadow(color: .black.opacity(0.5), radius: 24, y: 10)
    }

    // MARK: - Left: track info

    private var trackInfo: some View {
        HStack(spacing: 12) {
            if trackTitle != nil {
                Button(action: onNavigateToAlbum) {
                    albumArt
                        .frame(width: 54, height: 54)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        .shadow(color: .black.opacity(0.4), radius: 8, y: 4)
                        .accessibilityLabel("Album art")
                }
                .buttonStyle(.plain)
            }

            VStack(alignment: .leading, spacing: 2) {
                if let title = trackTitle {
                    Button(action: onNavigateToAlbum) {
                        Text(title)
                            .font(.system(size: 15, weight: .bold))
                            .tracking(-0.2)
                            .lineLimit(1)
                    }
                    .buttonStyle(.plain)
                }

                if let secondaryLine {
                    Text(secondaryLine)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
    }

    private var albumArt: some View {
        ImageView(content: cover, pointSize: 54)
    }

    // MARK: - Center: transport controls + progress

    private var centerColumn: some View {
        VStack(spacing: 10) {
            HStack(spacing: 22) {
                shuffleButton

                navButton(
                    "backward.fill",
                    help: "Previous track",
                    action: onPrevious
                )

                playPauseButton

                navButton(
                    "forward.fill",
                    help: "Next track",
                    action: onNext
                )

                repeatButton
            }

            progressBar
        }
    }

    private var shuffleButton: some View {
        let enabled = shuffled != nil
        let active = shuffled == true
        return Button {
            onSetShuffle(!(shuffled ?? false))
        } label: {
            toggleLabel("shuffle", active: active)
        }
        .buttonStyle(IconHoverButtonStyle())
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.4)
        .help(active ? "Turn off shuffle" : "Shuffle")
        .accessibilityLabel(active ? "Turn off shuffle" : "Shuffle")
    }

    private var repeatButton: some View {
        let active = repeatMode != .off
        let glyph = repeatMode == .track ? "repeat.1" : "repeat"
        return Button(action: onCycleRepeat) {
            toggleLabel(glyph, active: active)
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(repeatHelp)
        .accessibilityLabel(repeatHelp)
    }

    /// Shuffle/repeat share the same 30pt slot: accent glyph on a soft accent
    /// fill when active, secondary otherwise (hover brightening supplied by
    /// `IconHoverButtonStyle`).
    @ViewBuilder
    private func toggleLabel(_ systemName: String, active: Bool) -> some View {
        let glyph = Image(systemName: systemName)
            .font(.system(size: 16, weight: .semibold))
        Group {
            if active {
                glyph.foregroundStyle(Theme.accent)
            }
            else {
                glyph
            }
        }
        .frame(width: 30, height: 30)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(active ? Theme.accentSoft : Color.clear),
        )
        .contentShape(Rectangle())
    }

    private func navButton(
        _ systemName: String,
        help: LocalizedStringKey,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 20, weight: .medium))
                .frame(width: 34, height: 34)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(help)
        .accessibilityLabel(help)
    }

    private var playPauseButton: some View {
        PlayPauseControl(
            isPlaying: isPlaying,
            isLoading: isLoading,
            glyphFont: .system(size: 18, weight: .medium),
            spinnerControlSize: .small,
            targetSize: 48,
            onToggle: onPlayPause,
        )
        .foregroundStyle(Color(white: 0.12))
        // The window pins the dark scheme, but this control sits on a light
        // circle: force the light scheme so the loading spinner (an
        // NSProgressIndicator, which ignores `.tint`) draws dark on it.
        .environment(\.colorScheme, .light)
        .background(Circle().fill(Color(white: 0.97)))
        .shadow(color: .black.opacity(0.5), radius: 9, y: 5)
        .scaleEffect(playHovering ? 1.05 : 1.0)
        .onHover { hovering in
            withAnimation(.easeInOut(duration: 0.12)) {
                playHovering = hovering
            }
        }
    }

    private var progressBar: some View {
        PlaybackProgressRepresentable(
            showRemainingTime: showRemainingTime,
            durationMs: durationMs,
            onSeek: onSeek,
            onToggleRemainingTime: onToggleRemainingTime,
        )
        .frame(maxWidth: .infinity)
        .frame(height: 20)
        .accessibilityLabel("Playback position")
    }

    // MARK: - Right: queue + volume

    private var trailingControls: some View {
        HStack(spacing: 12) {
            queueButton

            muteButton

            SlimSlider(value: volume, onChange: onVolumeChange)
                .frame(width: 96)
                .accessibilityLabel("Volume")
        }
    }

    private var queueButton: some View {
        Button(action: { showQueue.toggle() }) {
            queueGlyph
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help("Queue")
        .accessibilityLabel("Queue")
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(
                    queueButtonDropTargeted
                        ? Theme.accent.opacity(0.3) : Color.clear
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
                    fill: Theme.accent,
                    offset: CGSize(width: 6, height: -6)
                )
            )
        }
    }

    @ViewBuilder
    private var queueGlyph: some View {
        let glyph = Image(systemName: "list.bullet")
            .font(.system(size: 17, weight: .medium))
        if showQueue {
            glyph.foregroundStyle(Theme.accent)
        }
        else {
            glyph
        }
    }

    private var muteButton: some View {
        Button(action: onToggleMute) {
            Image(systemName: muteIconName)
                .font(.system(size: 15, weight: .medium))
                .frame(width: 30, height: 30)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(isMuted ? "Unmute" : "Mute")
        .accessibilityLabel(isMuted ? "Unmute" : "Mute")
    }

    /// The rendered volume level chooses the speaker glyph: silenced when muted
    /// or at zero, one wave up to the midpoint, two above it.
    private var muteIconName: String {
        if isMuted || volume == 0 {
            return "speaker.slash.fill"
        }
        return volume <= 0.55 ? "speaker.wave.1.fill" : "speaker.wave.2.fill"
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

/// Hover treatment shared by every icon-only button in the bar: a rounded
/// subtle fill while the pointer is over it, and the glyph stepping from the
/// secondary color toward the primary. Buttons that carry an active-state tint
/// (shuffle/repeat/queue) set their own glyph color, which wins over this base.
private struct IconHoverButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Hovering(configuration: configuration)
    }

    private struct Hovering: View {
        let configuration: Configuration
        @State
        private var hovering = false

        var body: some View {
            configuration.label
                .foregroundStyle(hovering ? Color.primary : Color.secondary)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.white.opacity(hovering ? 0.06 : 0)),
                )
                .opacity(configuration.isPressed ? 0.6 : 1)
                .onHover { hovering = $0 }
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
        let shuffled: Bool?

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
                showRemainingTime: false,
                volume: volume,
                isMuted: isMuted,
                repeatMode: repeatMode,
                shuffled: shuffled,
                showQueue: $showQueue,
                onPlayPause: {},
                onNext: {},
                onPrevious: {},
                onSeek: { _ in },
                onToggleRemainingTime: {},
                onVolumeChange: { volume = $0 },
                onToggleMute: { isMuted.toggle() },
                onSetShuffle: { _ in },
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
            shuffled: false,
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
            shuffled: false,
        )
        .environment(MediaPaths.stub)
        .environment(PreviewData.queueStore(manualCount: 2, shuffled: true))
        .environment(Queue.stub)
    }

    #Preview("Shuffle active") {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: true,
            repeatMode: .off,
            shuffled: true,
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
            shuffled: nil,
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
            shuffled: nil,
        )
        .environment(MediaPaths.stub)
        .environment(PreviewData.queueStore(manualCount: 0, context: nil))
        .environment(Queue.stub)
    }
#endif
