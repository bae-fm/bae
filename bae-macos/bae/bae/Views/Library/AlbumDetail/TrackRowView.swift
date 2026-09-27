import BaeKit
import SwiftUI

/// One row in an album's track list.
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
            // Every leading-slot state stays in the layout, opacity-toggled,
            // so the row's size never changes.
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
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(isHovered ? Theme.hover : Color.clear)
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(Theme.accent.opacity(highlightOpacity))
            }
        )
        // The hover fill bleeds past the text column so the text stays aligned
        // with the header above the list.
        .padding(.horizontal, -10)
        // Keyed on the flash's `seq` so a remounted row still sees the flash
        // and a repeat navigation fires it again.
        .task(id: uiStore.pendingTrackFlash?.seq) {
            guard let flash = uiStore.pendingTrackFlash,
                flash.trackId == track.id
            else {
                return
            }
            highlightOpacity = ThemeOpacity.tintStrong
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

#if DEBUG
    @MainActor
    private func previewTrackRow(
        track: Track,
        artist: String? = nil,
        isCurrent: Bool = false,
        isLoading: Bool = false,
        isPlaying: Bool = false
    ) -> some View {
        TrackRowView(
            track: track,
            artist: artist,
            isCurrent: isCurrent,
            isLoading: isLoading,
            isPlaying: isPlaying,
            onPlay: {},
            onTogglePlayPause: {},
            onAddNext: { _ in },
            onAddToQueue: { _ in },
            onExportTrack: { _ in },
        )
        .frame(height: 40)
    }

    #Preview("Track Row") {
        VStack(spacing: 0) {
            // Resting.
            previewTrackRow(
                track: PreviewData.previewTrack(
                    title: "Track Title",
                    position: "1"
                )
            )
            // Current + playing.
            previewTrackRow(
                track: PreviewData.previewTrack(
                    title: "Track Title",
                    position: "2"
                ),
                isCurrent: true,
                isPlaying: true
            )
            // Current + paused.
            previewTrackRow(
                track: PreviewData.previewTrack(
                    title: "Track Title",
                    position: "3"
                ),
                isCurrent: true,
                isPlaying: false
            )
            // Loading.
            previewTrackRow(
                track: PreviewData.previewTrack(
                    title: "Track Title",
                    position: "4"
                ),
                isLoading: true
            )
            // Compilation row: core supplies a per-track display artist.
            previewTrackRow(
                track: PreviewData.previewTrack(
                    title: "Track Title",
                    position: "5",
                    displayArtist: "Featured Artist"
                ),
                artist: "Featured Artist"
            )
            .frame(height: 52)
        }
        .padding(24)
        .frame(width: 460)
        .background(Theme.background)
        .environment(UiStore())
        .preferredColorScheme(.dark)
    }
#endif
