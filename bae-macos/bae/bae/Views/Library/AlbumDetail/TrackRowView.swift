import BaeKit
import SwiftUI

/// One row in an album's track list.
struct TrackRowView: View {
    @Environment(UiStore.self)
    private var uiStore
    let track: Track
    /// The artist to show, or `nil` for none — core's decision.
    let artist: String?
    /// The widest position label in the release, so every row's number
    /// column is as wide and no label wraps.
    let positionWidth: CGFloat
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

    /// The leading slot's least width: the play button and the loading
    /// spinner that take the number's place.
    fileprivate static let leadingSlotMinWidth: CGFloat = 22

    var body: some View {
        let isCurrentPlaying = isCurrent && isPlaying
        HStack(spacing: ThemeSpace.group) {
            // Every leading-slot state stays in the layout, opacity-toggled,
            // so the row's size never changes.
            ZStack {
                TrackPositionLabel(text: track.positionText)
                    .foregroundStyle(.tertiary)
                    .opacity(!isCurrent && !isHovered && !isLoading ? 1 : 0)

                Button(action: isCurrent ? onTogglePlayPause : onPlay) {
                    Image(
                        systemName: isCurrentPlaying
                            ? "pause.fill" : "play.fill"
                    )
                    .themeIcon(.medium)
                }
                .buttonStyle(.plain)
                .opacity(isHovered && !isLoading ? 1 : 0)
                .allowsHitTesting(isHovered && !isLoading)

                Image(systemName: "speaker.wave.2.fill")
                    .themeIcon(.small)
                    .foregroundStyle(Theme.accent)
                    .opacity(isCurrent && !isHovered && !isLoading ? 1 : 0)

                ProgressView()
                    .controlSize(.small)
                    .opacity(isLoading ? 1 : 0)
                    .allowsHitTesting(isLoading)
            }
            .frame(width: max(positionWidth, Self.leadingSlotMinWidth))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(track.title)
                    .themeText(.rowTitle)
                    .foregroundStyle(isCurrent ? Theme.accent : .primary)
                    .lineLimit(1)
                if let artist {
                    Text(artist)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            if !track.durationLabel.isEmpty {
                Text(track.durationLabel)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, ThemeSpace.related)
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
        .padding(.horizontal, -ThemeSpace.related)
        .revealsAsTrackRow(track.id)
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
}

/// A track's position as the track list draws it, on one line: the rows show
/// it, and the list measures every one to size the rows' number column.
struct TrackPositionLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.detail)
            .monospacedDigit()
            .lineLimit(1)
            .fixedSize()
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
            // A single-digit position measures under the slot's least width,
            // which the list's rows then take.
            positionWidth: TrackRowView.leadingSlotMinWidth,
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
        .padding(ThemeSpace.section)
        .frame(width: 460)
        .background(Theme.background)
        .environment(UiStore())
        .preferredColorScheme(.dark)
    }
#endif
