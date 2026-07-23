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
                    Text(group.sideHeaderText)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                        .padding(.top, 12)
                        .padding(.bottom, 4)
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
            }
            if !detail.totalDurationText.isEmpty {
                Text(detail.totalDurationText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.top, 12)
            }
        }
    }
}

private struct TrackRow: View {
    let track: Track
    /// The artist to show, or `nil` for none. Resolved by the album/work-release
    /// display; the row does not decide.
    let artist: String?
    let onPlay: () -> Void
    let onPlayNext: () -> Void
    let onAddToQueue: () -> Void

    // Read playback state at the leaf so only the rows whose indicator actually
    // changes re-render, rather than threading a snapshot down from the parent.
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
            HStack(spacing: 12) {
                // Both stay in the layout tree, toggled by opacity, so swapping
                // the current row in/out never re-measures the stack.
                ZStack(alignment: .leading) {
                    Text(track.positionText.isEmpty ? "-" : track.positionText)
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .opacity(isCurrent ? 0 : 1)
                    Image(
                        systemName: playbackStore.nowPlaying.isPlaying
                            ? "speaker.wave.2.fill" : "speaker.fill"
                    )
                    .font(.callout)
                    .foregroundStyle(Theme.accent)
                    .opacity(isCurrent ? 1 : 0)
                }
                .frame(width: 36, alignment: .leading)
                VStack(alignment: .leading, spacing: 2) {
                    Text(track.title)
                        .font(.body)
                        .foregroundStyle(isCurrent ? Theme.accent : .primary)
                        .lineLimit(1)
                    if let artist {
                        Text(artist)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
                if !track.durationLabel.isEmpty {
                    Text(track.durationLabel)
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            .contentShape(Rectangle())
            .padding(.vertical, 8)
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
