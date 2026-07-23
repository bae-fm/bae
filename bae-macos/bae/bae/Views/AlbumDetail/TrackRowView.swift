import BaeKit
import SwiftUI

/// One row in an album's track list. Its single leading slot carries the track
/// number and everything that stands in for it — hover play/pause, playing
/// speaker, loading spinner — all kept in the layout tree and opacity-toggled so
/// the row's intrinsic size never changes and sibling rows don't re-measure.
struct TrackRowView: View {
    @Environment(UiStore.self)
    private var uiStore
    let track: Track
    /// The artist to show, or `nil` for none — core's decision.
    let artist: String?
    let isCurrent: Bool
    let isLoading: Bool
    let isPlaying: Bool
    let onPlay: () -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void

    @State
    private var isHovered = false
    @State
    private var hoverWorkItem: DispatchWorkItem?
    @State
    private var highlightOpacity: Double = 0

    var body: some View {
        let isCurrentPlaying = isCurrent && isPlaying
        HStack(spacing: 14) {
            // One leading slot carries the track number and everything that
            // stands in for it — the hover play/pause, the playing speaker,
            // the loading spinner. All stay in the layout tree, opacity-
            // toggled, so the row's intrinsic size never changes and sibling
            // rows don't re-measure.
            ZStack {
                trackNumberLabel
                    .font(.system(size: 13, weight: .medium).monospacedDigit())
                    .foregroundStyle(.tertiary)
                    .opacity(!isCurrent && !isHovered && !isLoading ? 1 : 0)

                Button(action: isCurrent ? onTogglePlayPause : onPlay) {
                    Image(
                        systemName: isCurrentPlaying
                            ? "pause.fill" : "play.fill"
                    )
                    .font(.system(size: 12, weight: .semibold))
                }
                .buttonStyle(.plain)
                .opacity(isHovered && !isLoading ? 1 : 0)
                .allowsHitTesting(isHovered && !isLoading)

                Image(systemName: "speaker.wave.2.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.accent)
                    .opacity(isCurrent && !isHovered && !isLoading ? 1 : 0)

                ProgressView()
                    .controlSize(.small)
                    .opacity(isLoading ? 1 : 0)
                    .allowsHitTesting(isLoading)
            }
            .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(isCurrent ? Theme.accent : .primary)
                    .lineLimit(1)
                if let artist {
                    Text(artist)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            if !track.durationLabel.isEmpty {
                Text(track.durationLabel)
                    .font(
                        .system(size: 12.5, weight: .medium).monospacedDigit()
                    )
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 10)
        .frame(maxHeight: .infinity)
        .background(
            ZStack {
                RoundedRectangle(cornerRadius: 8)
                    .fill(.white.opacity(isHovered ? 0.05 : 0))
                RoundedRectangle(cornerRadius: 8)
                    .fill(Theme.accent.opacity(highlightOpacity))
            }
        )
        // The row chrome (the hover fill) bleeds past the text column so the
        // content stays aligned with the header block above the list.
        .padding(.horizontal, -10)
        // Keyed on the flash's `seq` (not a subject) for the same reason the
        // grid scroll is: navigating here can remount this row, and durable
        // state survives that where a one-shot emit would be lost. Re-fires when
        // seq changes (repeat navigation). Independent of the grid-scroll
        // consumer sharing the same navigateToAlbum call — each clears only its
        // own pending command, so one consumer can't starve the other.
        .task(id: uiStore.pendingTrackFlash?.seq) {
            guard let flash = uiStore.pendingTrackFlash,
                flash.trackId == track.id
            else {
                return
            }
            highlightOpacity = 0.3
            withAnimation(.easeOut(duration: 3)) {
                highlightOpacity = 0
            }
            uiStore.consumeTrackFlash(seq: flash.seq)
        }
        .onTapGesture(count: 2) {
            onPlay()
        }
        .contentShape(Rectangle())
        .onHover { hovering in
            hoverWorkItem?.cancel()
            if hovering {
                let item = DispatchWorkItem { isHovered = true }
                hoverWorkItem = item
                DispatchQueue.main.asyncAfter(
                    deadline: .now() + 0.05,
                    execute: item
                )
            }
            else {
                isHovered = false
                hoverWorkItem = nil
            }
        }
        .contextMenu {
            Button("Play") { onPlay() }
            Button("Play Next") { onAddNext(track.id) }
            Button("Add to Queue") { onAddToQueue(track.id) }
            Divider()
            Button("Save As…") { onExportTrack(track.id) }
        }
        .draggable(track.id)
    }

    private var trackNumberLabel: some View {
        Text(track.positionText)
    }
}
