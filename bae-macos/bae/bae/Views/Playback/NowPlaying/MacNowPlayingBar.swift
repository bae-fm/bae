import BaeKit
import Combine
import SwiftUI

/// The docked playback strip; `MacNowPlayingBarContainer` owns the state and wires
/// every callback.
struct MacNowPlayingBar: View {
    let trackTitle: String?
    let secondaryLine: String?
    let cover: ImageContent?
    let isPlaying: Bool
    /// Where the playing track is; `nil` while no track plays.
    let position: LatestSource<BridgeTrackPosition>?
    /// The parts of the playing track whose bytes are here; `nil` while no
    /// track plays.
    let buffered: LatestSource<BridgeTrackBuffered>?
    /// Whether the loading track's load waits on storage.
    let waitsOnStorage: Bool
    /// The user's elapsed-vs-remaining choice, from the config.
    let showRemainingTime: Bool
    /// The volume, mute and repeat mode; `nil` until core says them, when
    /// their controls keep their place and show nothing.
    let settings: PlaybackSettings?
    /// The name of the receiver playback is on when it takes no volume from
    /// bae; mute and the slider then give way to a line saying where the
    /// volume is set.
    let receiverWithoutVolume: String?
    /// Whether shuffle is on, with or without a playing context.
    let shuffled: Bool
    let showQueue: Bool
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void
    /// The level a volume drag is at: moves the output, stores nothing.
    let onVolumeDrag: (Float) -> Void
    /// The level a volume drag or click ends on.
    let onVolumeChange: (Float) async -> Void
    /// Mute or unmute: the value the mute button sets.
    let onSetMuted: (Bool) -> Void
    let onSetShuffle: (Bool) -> Void
    let onSetQueuePresented: (Bool) -> Void
    /// The repeat mode the repeat button steps to.
    let onSetRepeatMode: (BridgeRepeatMode) -> Void
    let onDropToQueue: ([String]) -> Void
    let onNavigateToAlbum: () -> Void
    let queueAddPublisher: AnyPublisher<Int, Never>
    /// The Cast control from the container; an empty view in shots.
    let castControl: AnyView

    @State
    private var queueButtonDropTargeted = false

    var body: some View {
        HStack(spacing: ThemeSpace.section) {
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
        // chrome, a hairline along its top edge.
        .background(Theme.surface)
        .overlay(alignment: .top) {
            MacHairline()
        }
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
                .buttonStyle(.accentPlain)
            }

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                if let title = trackTitle {
                    Button(action: onNavigateToAlbum) {
                        Text(title)
                            .themeText(.rowTitle)
                            .lineLimit(1)
                    }
                    .buttonStyle(.accentPlain)
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
        Artwork(content: cover, pointSize: ThemeSize.barArtwork)
    }

    // MARK: - Center: transport controls + progress

    private var centerColumn: some View {
        VStack(spacing: ThemeSpace.related) {
            MacTransport(
                isPlaying: isPlaying,
                waitsOnStorage: waitsOnStorage,
                shuffled: shuffled,
                repeatMode: settings?.repeatMode,
                onPlayPause: onPlayPause,
                onNext: onNext,
                onPrevious: onPrevious,
                onSetShuffle: onSetShuffle,
                onSetRepeatMode: onSetRepeatMode
            )

            progressBar
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
}

// MARK: - Right: queue + volume

extension MacNowPlayingBar {
    fileprivate var trailingControls: some View {
        HStack(spacing: ThemeSpace.group) {
            castControl

            queueButton

            switch volumeControls {
            case .setOnReceiver(let name):
                Text("Volume is set on \(name)")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            case .controls(let settings):
                muteButton(settings)

                MacVolumeSlider(
                    value: settings.volume,
                    onDrag: onVolumeDrag,
                    onChange: onVolumeChange
                )
                .frame(width: volumeWidth)
                .accessibilityLabel("Volume")
            case .waitingForCore:
                MacTransport.controlSpace

                Color.clear.frame(width: volumeWidth, height: 1)
            }
        }
    }

    fileprivate var queueButton: some View {
        MacIconButton(symbol: "list.bullet", label: "Queue", active: showQueue)
        {
            onSetQueuePresented(!showQueue)
        }
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

    /// What the bar's right edge offers for the volume.
    enum VolumeControls: Equatable {
        /// The receiver playback is on keeps its own volume: a line names it
        /// in place of mute and the slider, which could only be refused.
        case setOnReceiver(String)
        case controls(PlaybackSettings)
        /// Core has not said the settings yet: the controls keep their place.
        case waitingForCore
    }

    var volumeControls: VolumeControls {
        if let receiverWithoutVolume {
            return .setOnReceiver(receiverWithoutVolume)
        }
        if let settings {
            return .controls(settings)
        }
        return .waitingForCore
    }

    fileprivate var volumeWidth: CGFloat { 96 }

    fileprivate func muteButton(_ settings: PlaybackSettings) -> some View {
        let isMuted = settings.isMuted
        return MacIconButton(
            symbol: Self.muteIconName(settings),
            label: isMuted ? "Unmute" : "Mute"
        ) {
            onSetMuted(!isMuted)
        }
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
