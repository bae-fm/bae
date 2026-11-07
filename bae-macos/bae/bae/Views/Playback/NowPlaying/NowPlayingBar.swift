import BaeKit
import Combine
import SwiftUI

/// The docked playback strip; `NowPlayingBarContainer` owns the state and wires
/// every callback.
struct NowPlayingBar: View {
    let trackTitle: String?
    let secondaryLine: String?
    let cover: ImageContent?
    let isPlaying: Bool
    /// Where the playing track is; `nil` while no track plays.
    let position: LatestSource<BridgeTrackPosition>?
    /// The parts of the playing track whose bytes are here; `nil` while no
    /// track plays.
    let buffered: LatestSource<BridgeTrackBuffered>?
    /// Whether the loading track's load waits on storage; `nil` while no
    /// track loads.
    let storageWait: LatestSource<Bool>?
    /// The user's elapsed-vs-remaining choice, from the config.
    let showRemainingTime: Bool
    /// The volume, mute and repeat mode; `nil` until core says them, when
    /// their controls keep their place and show nothing.
    let settings: PlaybackSettings?
    /// The playing context's shuffle state; `nil` with no context, which
    /// disables the shuffle button.
    let shuffled: Bool?
    let showQueue: Bool
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void
    /// The level a volume drag is at: moves the output, stores nothing.
    let onVolumeDrag: (Float) -> Void
    /// The level a volume drag or click ends on.
    let onVolumeChange: (Float) -> Void
    /// Mute or unmute: the value the mute button sets.
    let onSetMuted: (Bool) -> Void
    let onSetShuffle: (Bool) -> Void
    let onSetQueuePresented: (Bool) -> Void
    /// The repeat mode the repeat button steps to.
    let onSetRepeatMode: (BridgeRepeatMode) -> Void
    let onDropToQueue: ([String]) -> Void
    let onNavigateToAlbum: () -> Void
    let queueAddPublisher: AnyPublisher<Int, Never>
    /// The Cast control from the container; an empty view in previews.
    let castControl: AnyView

    @State
    private var queueButtonDropTargeted = false

    var body: some View {
        HStack(spacing: 0) {
            trackInfo
                .frame(maxWidth: .infinity, alignment: .leading)

            centerColumn
                .frame(width: 460)

            trailingControls
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding(.vertical, ThemeSpace.group)
        .padding(.horizontal, ThemeSpace.section)
        // Docked in the window's bottom row, so a flat surface with no card
        // chrome.
        .background(Theme.surface)
    }

    // MARK: - Left: track info

    private var trackInfo: some View {
        HStack(spacing: ThemeSpace.group) {
            if trackTitle != nil {
                Button(action: onNavigateToAlbum) {
                    albumArt
                        .frame(
                            width: ThemeSize.barArtwork,
                            height: ThemeSize.barArtwork
                        )
                        .clipShape(
                            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        )
                        .shadow(color: Theme.shadow, radius: 8, y: 4)
                        .accessibilityLabel("Album art")
                }
                .buttonStyle(.plain)
            }

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                if let title = trackTitle {
                    Button(action: onNavigateToAlbum) {
                        Text(title)
                            .themeText(.rowTitle)
                            .lineLimit(1)
                    }
                    .buttonStyle(.plain)
                }

                if let secondaryLine {
                    Text(secondaryLine)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
    }

    private var albumArt: some View {
        ImageView(content: cover, pointSize: ThemeSize.barArtwork)
    }

    // MARK: - Center: transport controls + progress

    private var centerColumn: some View {
        VStack(spacing: ThemeSpace.related) {
            HStack(spacing: ThemeSpace.section) {
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

                if let settings {
                    repeatButton(settings.repeatMode)
                }
                else {
                    controlSpace
                }
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

    private func repeatButton(_ repeatMode: BridgeRepeatMode) -> some View {
        let active = repeatMode != .off
        let glyph = repeatMode == .track ? "repeat.1" : "repeat"
        let help = Self.repeatHelp(repeatMode)
        return Button {
            onSetRepeatMode(bridgeNextRepeatMode(mode: repeatMode))
        } label: {
            toggleLabel(glyph, active: active)
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(help)
        .accessibilityLabel(help)
    }

    /// The room one icon control takes, holding a control's place while
    /// there is nothing to show in it.
    fileprivate var controlSpace: some View {
        Color.clear
            .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
    }

    /// The slot shuffle and repeat share: an accent glyph on an accent fill
    /// when active.
    @ViewBuilder
    private func toggleLabel(_ systemName: String, active: Bool) -> some View {
        let glyph = Image(systemName: systemName)
            .themeIcon(.large)
        Group {
            if active {
                glyph.foregroundStyle(Theme.accent)
            }
            else {
                glyph
            }
        }
        .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
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
                .themeIcon(.large)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(help)
        .accessibilityLabel(help)
    }

    private var playPauseButton: some View {
        PlayPauseControl(
            isPlaying: isPlaying,
            glyphFont: ThemeIcon.large.font,
            targetSize: 48,
            onToggle: onPlayPause,
        )
        .foregroundStyle(.primary)
        .background(Circle().fill(Theme.tile))
        .overlay {
            if let storageWait {
                StorageWaitArc(source: storageWait)
            }
        }
    }

    /// The seek bar, which draws the parts of the track whose bytes are here
    /// under the played part.
    private var progressBar: some View {
        PlaybackProgressView(
            position: position,
            buffered: buffered,
            showRemainingTime: showRemainingTime,
            onSeek: onSeek,
            onToggleRemainingTime: onToggleRemainingTime,
        )
        .frame(maxWidth: .infinity)
        .frame(height: 20)
        .accessibilityLabel("Playback position")
    }

    private static func repeatHelp(
        _ repeatMode: BridgeRepeatMode
    ) -> LocalizedStringKey {
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

// MARK: - Right: queue + volume

extension NowPlayingBar {
    fileprivate var trailingControls: some View {
        HStack(spacing: ThemeSpace.group) {
            castControl

            queueButton

            if let settings {
                muteButton(settings)

                SlimSlider(
                    value: settings.volume,
                    onDrag: onVolumeDrag,
                    onChange: onVolumeChange
                )
                .frame(width: volumeWidth)
                .accessibilityLabel("Volume")
            }
            else {
                controlSpace

                Color.clear.frame(width: volumeWidth, height: 1)
            }
        }
    }

    fileprivate var queueButton: some View {
        Button(action: { onSetQueuePresented(!showQueue) }) {
            queueGlyph
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help("Queue")
        .accessibilityLabel("Queue")
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(
                    queueButtonDropTargeted
                        ? Theme.accentStrong : Color.clear
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
                    fill: Theme.accent,
                    offset: CGSize(
                        width: ThemeSpace.compact,
                        height: -ThemeSpace.compact
                    )
                )
            )
        }
    }

    @ViewBuilder
    fileprivate var queueGlyph: some View {
        let glyph = Image(systemName: "list.bullet")
            .themeIcon(.large)
        if showQueue {
            glyph.foregroundStyle(Theme.accent)
        }
        else {
            glyph
        }
    }

    fileprivate var volumeWidth: CGFloat { 96 }

    fileprivate func muteButton(_ settings: PlaybackSettings) -> some View {
        let isMuted = settings.isMuted
        return Button {
            onSetMuted(!isMuted)
        } label: {
            Image(systemName: Self.muteIconName(settings))
                .themeIcon(.large)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(isMuted ? "Unmute" : "Mute")
        .accessibilityLabel(isMuted ? "Unmute" : "Mute")
    }

    /// The speaker glyph for the volume: silenced when muted or at zero, one
    /// wave up to 0.55, two above it.
    fileprivate static func muteIconName(_ settings: PlaybackSettings) -> String
    {
        if settings.isMuted || settings.volume == 0 {
            return "speaker.slash.fill"
        }
        return settings.volume <= 0.55
            ? "speaker.wave.1.fill" : "speaker.wave.2.fill"
    }
}

#if DEBUG
    // MARK: - Previews

    /// Preview host that holds the queue, volume, and mute state.
    struct NowPlayingBarPreview: View {
        let trackTitle: String?
        let artistNames: String?
        let isPlaying: Bool
        /// The parts of the track whose bytes are here.
        var held = [BridgeTrackSpan(start: 0, end: 1)]
        /// Whether the track's load waits on storage.
        var waitsOnStorage = false
        let repeatMode: BridgeRepeatMode
        let shuffled: Bool?
        /// The seek bar's position: how far through a track, or none.
        var hasPosition = true
        var progress = 0.25

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
                position: hasPosition
                    ? PreviewData.trackPosition(
                        positionMs: Int64(progress * 222_000),
                        durationMs: 222_000
                    )
                    : nil,
                buffered: hasPosition
                    ? LatestStream.trackBuffered(
                        holding: [
                            "preview-track": BridgeTrackBuffered(ranges: held)
                        ]
                    )
                    .source(for: "preview-track")
                    : nil,
                storageWait:
                    LatestStream.trackStorageWaits(
                        waiting: waitsOnStorage ? ["preview-track"] : []
                    )
                    .source(for: "preview-track"),
                showRemainingTime: false,
                settings: PlaybackSettings(
                    volume: volume,
                    isMuted: isMuted,
                    repeatMode: repeatMode
                ),
                shuffled: shuffled,
                showQueue: showQueue,
                onPlayPause: {},
                onNext: {},
                onPrevious: {},
                onSeek: { _ in },
                onToggleRemainingTime: {},
                onVolumeDrag: { volume = $0 },
                onVolumeChange: { volume = $0 },
                onSetMuted: { isMuted = $0 },
                onSetShuffle: { _ in },
                onSetQueuePresented: { showQueue = $0 },
                onSetRepeatMode: { _ in },
                onDropToQueue: { _ in },
                onNavigateToAlbum: {},
                queueAddPublisher: Empty().eraseToAnyPublisher(),
                castControl: AnyView(EmptyView()),
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
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 2))
        .environment(Queue.stub())
    }

    #Preview("Paused — Repeat Context") {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: false,
            repeatMode: .context,
            shuffled: false,
        )
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 2, shuffled: true))
        .environment(Queue.stub())
    }

    #Preview("Shuffle active") {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: true,
            repeatMode: .off,
            shuffled: true,
        )
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 2, shuffled: true))
        .environment(Queue.stub())
    }

    #Preview("Waiting on storage") {
        PreviewScenes.nowPlayingBar(.waitingOnStorage)
    }

    #Preview("Playing with a gap") {
        PreviewScenes.nowPlayingBar(.playingWithAGap)
    }

    #Preview("All here") {
        PreviewScenes.nowPlayingBar(.allHere)
    }

    #Preview("Empty") {
        NowPlayingBarPreview(
            trackTitle: nil,
            artistNames: nil,
            isPlaying: false,
            repeatMode: .off,
            shuffled: nil,
            hasPosition: false,
        )
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 0, context: nil))
        .environment(Queue.stub())
    }
#endif
