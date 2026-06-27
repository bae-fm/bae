import Combine
import SwiftUI

/// Isolates observation so that only slow-changing properties (track info, volume, etc.)
/// trigger SwiftUI re-evaluation. Position ticks bypass SwiftUI entirely via the AppKit
/// PlaybackProgressNSView, which AppService updates directly.
struct NowPlayingBarContainer: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(PlaybackStore.self)
    var playbackStore
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(UiStore.self)
    var uiStore
    let onQueueInsertTracks: ([String], Int) -> Void
    let onDropToQueue: ([String]) -> Void

    var body: some View {
        @Bindable
        var uiStore = uiStore
        let np = playbackStore.nowPlaying
        let track = np.track
        // Resolve the cover from the observed album summary when the playing
        // album is in the library slice: its `cover` carries the content
        // version, so changing the cover moves the field and this bar
        // re-renders. Fall back to the track's bare cover image id for albums
        // not yet interned (the bar can show a track before its summary lands).
        let cover: ImageContent? =
            track.flatMap { libraryStore.albumSummaries[$0.albumId]?.cover }
            .map { .library(.cover(id: $0.id, version: $0.version)) }
            ?? track?.coverImageId
            .map { .library(.cover(id: $0, version: nil)) }
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
            queueIsActive: np.isActive,
            queueNowPlayingTitle: track?.trackTitle,
            queueNowPlayingArtist: track?.artistNames,
            queueNowPlayingCover: cover,
            queueManual: playbackStore.manualQueue,
            queueContext: playbackStore.queueContext,
            onPlayPause: { playback.togglePlayPause() },
            onNext: { playback.nextTrack() },
            onPrevious: { playback.previousTrack() },
            onSeek: { playback.seekByRatio($0) },
            onVolumeChange: { playback.setVolume($0) },
            onToggleMute: { playback.toggleMute() },
            onCycleRepeat: { playback.cycleRepeatMode() },
            onQueueClear: { queue.clearQueue() },
            onQueueSkipTo: { queue.skipToEntry($0) },
            onQueueRemove: { queue.removeEntry($0) },
            onQueueReorder: { entryId, beforeEntryId in
                queue.reorderEntry(entryId, beforeEntryId)
            },
            onQueueInsertTracks: onQueueInsertTracks,
            onQueueSetShuffle: { queue.setShuffle($0) },
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

/// Patches SwiftUI's .popover to use .applicationDefined behavior so it
/// stays open during drag operations. Dismiss via the queue button or X.
private struct StablePopoverBehavior: NSViewRepresentable {
    func makeNSView(context _: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            if let popover = view.window?.value(forKey: "_popover")
                as? NSPopover
            {
                popover.behavior = .applicationDefined
            }
        }
        return view
    }

    func updateNSView(_: NSView, context _: Context) {}
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
    let repeatMode: RepeatMode
    @Binding
    var showQueue: Bool
    let queueIsActive: Bool
    let queueNowPlayingTitle: String?
    let queueNowPlayingArtist: String?
    let queueNowPlayingCover: ImageContent?
    let queueManual: [QueueItem]
    let queueContext: QueuePlaybackContext?
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSeek: (Double) -> Void
    let onVolumeChange: (Float) -> Void
    let onToggleMute: () -> Void
    let onCycleRepeat: () -> Void
    let onQueueClear: () -> Void
    let onQueueSkipTo: (String) -> Void
    let onQueueRemove: (String) -> Void
    let onQueueReorder: (String, String?) -> Void
    let onQueueInsertTracks: ([String], Int) -> Void
    let onQueueSetShuffle: (Bool) -> Void
    let onDropToQueue: ([String]) -> Void
    let onNavigateToAlbum: () -> Void
    let queueAddPublisher: AnyPublisher<Int, Never>

    @State
    private var queueButtonDropTargeted = false
    @State
    private var queueAddDisplayedCount: Int?
    @State
    private var queueAddHideTask: Task<Void, Never>?

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
        .frame(height: 64)
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
                        .font(.body)
                }
                .buttonStyle(.plain)
                .help("Previous track")
                .accessibilityLabel("Previous track")

                PlayPauseControl(
                    isPlaying: isPlaying,
                    isLoading: isLoading,
                    glyphFont: .title2,
                    spinnerControlSize: .small,
                    onToggle: onPlayPause
                )

                Button(action: onNext) {
                    Image(systemName: "forward.fill")
                        .font(.body)
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
            }
            .buttonStyle(.plain)
            .font(.body)
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
            .popover(isPresented: $showQueue, arrowEdge: .bottom) {
                QueueView(
                    isActive: queueIsActive,
                    nowPlayingTitle: queueNowPlayingTitle,
                    nowPlayingArtist: queueNowPlayingArtist,
                    nowPlayingCover: queueNowPlayingCover,
                    manual: queueManual,
                    context: queueContext,
                    onClose: { showQueue = false },
                    onClear: { onQueueClear() },
                    onSkipTo: { onQueueSkipTo($0) },
                    onRemove: { onQueueRemove($0) },
                    onReorder: { onQueueReorder($0, $1) },
                    onInsertTracks: { onQueueInsertTracks($0, $1) },
                    onSetShuffle: { onQueueSetShuffle($0) },
                )
                .frame(width: 350, height: 500)
                .background { StablePopoverBehavior() }
            }
            .overlay(alignment: .topTrailing) {
                QueueAddBadge(displayedCount: $queueAddDisplayedCount)
                    .offset(x: 6, y: -6)
                    .allowsHitTesting(false)
            }
            .onReceive(queueAddPublisher) { count in
                handleQueueItemsAdded(count: count)
            }

            Button(action: onToggleMute) {
                Image(
                    systemName: isMuted ? "speaker.slash.fill" : "speaker.fill"
                )
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .font(.body)
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
        .buttonStyle(.plain)
        .font(.body)
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

    private func handleQueueItemsAdded(count: Int) {
        queueAddHideTask?.cancel()
        withAnimation(.spring(response: 0.3, dampingFraction: 0.6)) {
            queueAddDisplayedCount = count
        }
        queueAddHideTask = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(1400))
            if Task.isCancelled {
                return
            }
            withAnimation(.easeIn(duration: 0.2)) {
                queueAddDisplayedCount = nil
            }
        }
    }
}

// MARK: - Previews

/// Preview host that provides the state bindings NowPlayingBar needs.
/// The playback position publisher defaults to `Empty()` — the NSView
/// renders the duration clock from the static `durationMs` prop.
private struct NowPlayingBarPreview: View {
    let trackTitle: String?
    let artistNames: String?
    let isPlaying: Bool
    var isLoading: Bool = false
    let repeatMode: RepeatMode
    let queueIsActive: Bool
    var queueManual: [QueueItem] = []
    var queueContext: QueuePlaybackContext?

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
            queueIsActive: queueIsActive,
            queueNowPlayingTitle: trackTitle,
            queueNowPlayingArtist: artistNames,
            queueNowPlayingCover: nil,
            queueManual: queueManual,
            queueContext: queueContext,
            onPlayPause: {},
            onNext: {},
            onPrevious: {},
            onSeek: { _ in },
            onVolumeChange: { volume = $0 },
            onToggleMute: { isMuted.toggle() },
            onCycleRepeat: {},
            onQueueClear: {},
            onQueueSkipTo: { _ in },
            onQueueRemove: { _ in },
            onQueueReorder: { _, _ in },
            onQueueInsertTracks: { _, _ in },
            onQueueSetShuffle: { _ in },
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
        queueIsActive: true,
        queueManual: Array(PreviewData.queueItems.prefix(2)),
        queueContext: QueuePlaybackContext(
            kind: .release,
            shuffled: false,
            upcoming: Array(PreviewData.queueItems.suffix(3))
        ),
    )
    .environment(MediaPaths.stub)
}

#Preview("Paused — Repeat Context") {
    NowPlayingBarPreview(
        trackTitle: PreviewData.nowPlayingTitle,
        artistNames: PreviewData.nowPlayingArtist,
        isPlaying: false,
        repeatMode: .context,
        queueIsActive: true,
        queueManual: Array(PreviewData.queueItems.prefix(2)),
        queueContext: QueuePlaybackContext(
            kind: .release,
            shuffled: true,
            upcoming: Array(PreviewData.queueItems.suffix(3))
        ),
    )
    .environment(MediaPaths.stub)
}

#Preview("Loading") {
    NowPlayingBarPreview(
        trackTitle: PreviewData.nowPlayingTitle,
        artistNames: PreviewData.nowPlayingArtist,
        isPlaying: true,
        isLoading: true,
        repeatMode: .off,
        queueIsActive: true,
        queueManual: PreviewData.queueItems,
        queueContext: nil,
    )
    .environment(MediaPaths.stub)
}

#Preview("Empty") {
    NowPlayingBarPreview(
        trackTitle: nil,
        artistNames: nil,
        isPlaying: false,
        repeatMode: .off,
        queueIsActive: false,
    )
    .environment(MediaPaths.stub)
}
