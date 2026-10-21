import BaeKit
import SwiftUI

/// Side-grouped track list. A tap plays the release from that track, named by
/// its id.
struct TrackList: View {
    let detail: ReleaseDetail
    let artistDisplay: TrackArtistDisplay
    let onPlayTrack: (String) -> Void
    let onPlayNext: (String) -> Void
    let onAddToQueue: (String) -> Void

    /// The widest of the release's position labels, as measured where the
    /// list lays them out, so every row's number column is as wide.
    @State
    private var positionWidth: CGFloat = 0

    var body: some View {
        let groups = detail.trackGroups

        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                if !group.sideHeaderText.isEmpty {
                    Eyebrow(verbatim: group.sideHeaderText)
                        .padding(.top, ThemeSpace.group)
                        .padding(.bottom, ThemeSpace.inline)
                }
                ForEach(group.tracks, id: \.id) { track in
                    TrackRow(
                        track: track,
                        artist: artistDisplay.artist(for: track),
                        positionWidth: positionWidth,
                        onPlay: { onPlayTrack(track.id) },
                        onPlayNext: { onPlayNext(track.id) },
                        onAddToQueue: { onAddToQueue(track.id) }
                    )
                }
                // The album's play time sits in the header; only a multi-side
                // release needs each side's named here.
                if groups.count > 1, !group.totalDurationText.isEmpty {
                    Text(group.totalDurationText)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .padding(.top, ThemeSpace.related)
                }
            }
        }
        .background {
            // Every position label laid out but not drawn: the stack is as
            // wide as the widest.
            ZStack {
                ForEach(
                    Set(detail.tracks.map(TrackPositionLabel.text(for:)))
                        .sorted(),
                    id: \.self
                ) { text in
                    TrackPositionLabel(text: text)
                }
            }
            .hidden()
            .onGeometryChange(for: CGFloat.self) { geometry in
                geometry.size.width
            } action: { width in
                positionWidth = width
            }
        }
    }
}

/// A track's position as the track list draws it, on one line: the rows show
/// it, and the list measures every one to size the rows' number column.
private struct TrackPositionLabel: View {
    let text: String

    /// The label for `track`, a dash when it has no position.
    static func text(for track: Track) -> String {
        track.positionText.isEmpty ? "-" : track.positionText
    }

    var body: some View {
        Text(text)
            .monospacedDigit()
            .themeText(.body)
            .lineLimit(1)
            .fixedSize()
    }
}

private struct TrackRow: View {
    let track: Track
    /// The artist to show, or `nil` for none, as the album display resolves it.
    let artist: String?
    /// The widest position label in the release, so every row's number
    /// column is as wide and no label wraps.
    let positionWidth: CGFloat
    let onPlay: () -> Void
    let onPlayNext: () -> Void
    let onAddToQueue: () -> Void

    // Read at the leaf so only rows whose indicator changes re-render.
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback

    /// The number column's least width, which the now-playing icon in the
    /// number's place also fits.
    private static let positionMinWidth: CGFloat = 36

    private var isCurrent: Bool {
        track.id == playbackStore.nowPlaying.track?.track.trackId
    }

    var body: some View {
        // Tapping the current track toggles play/pause; any other track plays
        // the release from there.
        Button {
            if isCurrent {
                playback.playPause(for: playbackStore.nowPlaying)
            }
            else {
                onPlay()
            }
        } label: {
            HStack(spacing: ThemeSpace.group) {
                // Both stay in the layout tree, toggled by opacity, so swapping
                // the current row in/out never re-measures the stack.
                ZStack(alignment: .leading) {
                    TrackPositionLabel(text: TrackPositionLabel.text(for: track))
                        .foregroundStyle(.secondary)
                        .opacity(isCurrent ? 0 : 1)
                    Image(
                        systemName: playbackStore.nowPlaying.isPlaying
                            ? "speaker.wave.2.fill" : "speaker.fill"
                    )
                    .themeIcon(.medium)
                    .foregroundStyle(Theme.accent)
                    .opacity(isCurrent ? 1 : 0)
                }
                .frame(
                    width: max(positionWidth, Self.positionMinWidth),
                    alignment: .leading
                )
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
                        .monospacedDigit()
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                }
            }
            .contentShape(Rectangle())
            .padding(.vertical, ThemeSpace.related)
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button {
                onPlayNext()
            } label: {
                Label("Play Next", systemImage: "text.insert")
            }
            Button {
                onAddToQueue()
            } label: {
                Label("Add to Queue", systemImage: "text.append")
            }
        }
    }
}

#if DEBUG
#Preview {
    ScrollView {
        TrackList(
            detail: PreviewData.releaseDetail,
            artistDisplay: .album,
            onPlayTrack: { _ in },
            onPlayNext: { _ in },
            onAddToQueue: { _ in }
        )
        .padding()
    }
    .previewStores()
}
#endif
