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
            // The reading order is the board's: the track, the transport, the
            // seek bar, then Queue, Mute and Volume. The seek bar is a
            // native view, which SwiftUI places by where it sits, so each
            // part says its own place in the order.
            trackInfo
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilitySortPriority(ReadingOrder.track)

            centerColumn
                .frame(width: 460)

            trailingControls
                .frame(maxWidth: .infinity, alignment: .trailing)
                .accessibilitySortPriority(ReadingOrder.rightControls)
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

    /// The cover, title and artist are one button that opens the playing
    /// album, named "Now playing, <title> by <artist>".
    @ViewBuilder
    private var trackInfo: some View {
        if let title = trackTitle {
            Button(action: onNavigateToAlbum) {
                trackLabel(title: title)
            }
            .buttonStyle(.accentPlain)
            .accessibilityElement(children: .ignore)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel(buttonName(title: title))
        }
        else {
            trackLabel(title: nil)
        }
    }

    private func buttonName(title: String) -> String {
        guard let secondaryLine else { return title }
        return String(localized: "Now playing, \(title) by \(secondaryLine)")
    }

    private func trackLabel(title: String?) -> some View {
        HStack(spacing: ThemeSpace.group) {
            if title != nil {
                albumArt
                    .frame(
                        width: ThemeSize.barArtwork,
                        height: ThemeSize.barArtwork
                    )
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    )
                    .shadow(color: Theme.shadow, radius: 8, y: 4)
            }

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                if let title {
                    Text(title)
                        .themeText(.rowTitle)
                        .lineLimit(1)
                }

                if let secondaryLine {
                    Text(secondaryLine)
                        .themeText(.detail)
                        .foregroundStyle(Theme.macTextSecondary)
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
            .accessibilitySortPriority(ReadingOrder.transport)

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
        .accessibilitySortPriority(ReadingOrder.seekBar)
    }
}

/// How early each part of the bar is read: a higher number is read first.
private enum ReadingOrder {
    static let track = 4.0
    static let transport = 3.0
    static let seekBar = 2.0
    static let rightControls = 1.0
}

// MARK: - Right: queue + volume

extension MacNowPlayingBar {
    fileprivate var trailingControls: some View {
        HStack(spacing: ThemeSpace.group) {
            castControl

            queueButton

            MacVolumeSlider(
                controls: volumeControls,
                onDrag: onVolumeDrag,
                onChange: onVolumeChange,
                onSetMuted: onSetMuted
            )
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

    var volumeControls: MacVolumeSlider.Controls {
        if let receiverWithoutVolume {
            return .setOnReceiver(receiverWithoutVolume)
        }
        if let settings {
            return .controls(settings)
        }
        return .waitingForCore
    }
}
