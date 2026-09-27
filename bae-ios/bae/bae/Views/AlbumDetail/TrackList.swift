import BaeKit
import SwiftUI

/// Side-grouped track list. Flattens groups to a release-wide index so a tap
/// maps to the ordered list the player builds from the same flattening.
struct TrackList: View {
    let detail: ReleaseDetail
    let artistDisplay: TrackArtistDisplay
    let onPlayTrackAt: (Int) -> Void
    let onPlayNext: (String) -> Void
    let onAddToQueue: (String) -> Void

    var body: some View {
        let groups = detail.trackGroups
        var runningOffset = 0
        let offsets = groups.map { group -> Int in
            let offset = runningOffset
            runningOffset += group.tracks.count
            return offset
        }

        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(groups.enumerated()), id: \.offset) {
                groupIndex,
                group in
                let groupOffset = offsets[groupIndex]
                if !group.sideHeaderText.isEmpty {
                    Eyebrow(verbatim: group.sideHeaderText)
                        .padding(.top, ThemeSpace.group)
                        .padding(.bottom, ThemeSpace.inline)
                }
                ForEach(Array(group.tracks.enumerated()), id: \.element.id) {
                    localIndex,
                    track in
                    TrackRow(
                        track: track,
                        artist: artistDisplay.artist(for: track),
                        onPlay: { onPlayTrackAt(groupOffset + localIndex) },
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
    }
}

private struct TrackRow: View {
    let track: Track
    /// The artist to show, or `nil` for none, as the album display resolves it.
    let artist: String?
    let onPlay: () -> Void
    let onPlayNext: () -> Void
    let onAddToQueue: () -> Void

    // Read at the leaf so only rows whose indicator changes re-render.
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Playback.self)
    private var playback

    private var isCurrent: Bool {
        track.id == playbackStore.nowPlaying.track?.trackId
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
                    Text(track.positionText.isEmpty ? "-" : track.positionText)
                        .monospacedDigit()
                        .themeText(.body)
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
                .frame(width: 36, alignment: .leading)
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
            onPlayTrackAt: { _ in },
            onPlayNext: { _ in },
            onAddToQueue: { _ in }
        )
        .padding()
    }
    .previewStores()
}
#endif
