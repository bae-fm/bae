import BaeKit
import SwiftUI

/// The track list under an album's detail card. Each side or disc lays out in
/// two column slots (core says where a side splits across both), so every row
/// gets one column's width and ends its duration at the same x.
struct MacTrackColumns: View {
    let release: ReleaseDetail
    let isCompilation: Bool
    let currentTrackId: String?
    let isPlaying: Bool
    let onPlayFromTrack: (String) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void
    /// Rate a track with these stars, or clear its rating with none; `nil` for
    /// a person who can't rate.
    let onRateTrack: (String, BridgeStars?) -> Void
    var playlists = TrackPlaylistChoices.none
    var picking: TrackPicking?

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
                    MacEyebrow(verbatim: group.sideHeaderText)
                        .padding(.top, groupIndex == 0 ? 0 : ThemeSpace.edge)
                        .padding(.bottom, ThemeSpace.compact)
                }
                HStack(alignment: .top, spacing: ThemeSpace.page) {
                    if let start = group.secondColumnStart {
                        trackColumn(tracks: Array(group.tracks.prefix(start)))
                        trackColumn(
                            tracks: Array(group.tracks.dropFirst(start))
                        )
                    }
                    else {
                        trackColumn(tracks: group.tracks)
                        // Not a `Spacer`: its default minimum length would
                        // claim its own width rather than take the share a
                        // sibling column takes.
                        Color.clear.frame(maxWidth: .infinity)
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
                    MacTrackLead(text: text)
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
                    onRate: { onRateTrack(track.id, $0) },
                    playlists: playlists,
                    picking: picking
                )
                .id(track.id)
                .frame(height: height)
            }
        }
    }
}
