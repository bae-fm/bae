import BaeKit
import SwiftUI

/// The track list under an album's detail card. Each side or disc lays out in
/// two column slots (a side over eight tracks splits across both), so every
/// row gets one column's width and ends its duration at the same x.
struct AlbumTrackListView: View {
    let release: ReleaseDetail
    let isCompilation: Bool
    let currentTrackId: String?
    let isPlaying: Bool
    let onPlayFromTrack: (String) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void
    var playlists = TrackPlaylistChoices.none

    @ScaledMetric(relativeTo: .body)
    private var rowHeight: CGFloat = 40
    @ScaledMetric(relativeTo: .body)
    private var rowHeightCompilation: CGFloat = 52
    /// The widest of the release's position labels, as measured where the
    /// list lays them out, so every row's number column is as wide.
    @State
    private var positionWidth: CGFloat = 0

    var body: some View {
        let groups = release.trackGroups

        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(groups.enumerated()), id: \.offset) {
                groupIndex,
                group in
                if !group.sideHeaderText.isEmpty {
                    Eyebrow(verbatim: group.sideHeaderText)
                        .padding(.top, groupIndex == 0 ? 0 : ThemeSpace.edge)
                        .padding(.bottom, ThemeSpace.compact)
                }
                let mid =
                    group.tracks.count > 8
                    ? (group.tracks.count + 1) / 2 : group.tracks.count
                let left = Array(group.tracks.prefix(mid))
                let right = Array(group.tracks.dropFirst(mid))
                HStack(alignment: .top, spacing: ThemeSpace.page) {
                    trackColumn(tracks: left)
                    if right.isEmpty {
                        // Not a `Spacer`: its default minimum length would
                        // claim its own width rather than take the share a
                        // sibling column takes.
                        Color.clear.frame(maxWidth: .infinity)
                    }
                    else {
                        trackColumn(tracks: right)
                    }
                }
                // The album's play time sits in the header; only a multi-side
                // release needs each side's named here.
                if group.showsTime, !group.totalDurationText.isEmpty {
                    Text(group.totalDurationText)
                        .themeText(.fine)
                        .foregroundStyle(.tertiary)
                        .padding(.top, ThemeSpace.related)
                }
            }
        }
        .background {
            // Every position label laid out but not drawn: the stack is as
            // wide as the widest.
            ZStack {
                ForEach(
                    Set(release.tracks.map(\.positionText)).sorted(),
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

    private func trackColumn(tracks: [Track]) -> some View {
        let height = isCompilation ? rowHeightCompilation : rowHeight
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(tracks, id: \.id) { track in
                TrackRowView(
                    track: track,
                    // Core sets this only for a compilation.
                    artist: track.displayArtist?.text,
                    positionWidth: positionWidth,
                    isCurrent: currentTrackId == track.id,
                    isPlaying: isPlaying,
                    onPlay: { onPlayFromTrack(track.id) },
                    onTogglePlayPause: onTogglePlayPause,
                    onAddNext: onAddNext,
                    onAddToQueue: onAddToQueue,
                    onExportTrack: onExportTrack,
                    playlists: playlists
                )
                .id(track.id)
                .frame(height: height)
            }
        }
    }
}
