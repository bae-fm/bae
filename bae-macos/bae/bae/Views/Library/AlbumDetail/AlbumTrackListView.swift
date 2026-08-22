import BaeKit
import SwiftUI

/// The track list under an album's detail card. Lays each side/disc group out
/// as one or two columns (splitting long sides in half), with side headers and
/// a total-duration footer. Delegates each row to `TrackRowView`.
struct AlbumTrackListView: View {
    let release: ReleaseDetail
    let isCompilation: Bool
    let currentTrackId: String?
    let loadingTrackId: String?
    let isPlaying: Bool
    let onPlayFromTrack: (Int) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void

    @ScaledMetric(relativeTo: .body)
    private var rowHeight: CGFloat = 40
    @ScaledMetric(relativeTo: .body)
    private var rowHeightCompilation: CGFloat = 52

    var body: some View {
        let groups = release.trackGroups
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
                let globalOffset = offsets[groupIndex]
                if !group.sideHeaderText.isEmpty {
                    Text(group.sideHeaderText)
                        .font(.system(size: 10, weight: .bold))
                        .tracking(1.2)
                        .textCase(.uppercase)
                        .foregroundStyle(.secondary)
                        .padding(.top, groupIndex == 0 ? 0 : 18)
                        .padding(.bottom, 6)
                }
                if group.tracks.count > 8 {
                    let mid = (group.tracks.count + 1) / 2
                    let left = Array(group.tracks.prefix(mid))
                    let right = Array(group.tracks.dropFirst(mid))
                    HStack(alignment: .top, spacing: 40) {
                        trackColumn(tracks: left, globalOffset: globalOffset)
                        trackColumn(
                            tracks: right,
                            globalOffset: globalOffset + mid
                        )
                    }
                }
                else {
                    trackColumn(
                        tracks: group.tracks,
                        globalOffset: globalOffset
                    )
                }
                // One group's runtime is the album's; only a multi-side release
                // needs each side named.
                if groups.count > 1, !group.totalDurationText.isEmpty {
                    Text(group.totalDurationText)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.tertiary)
                        .padding(.top, 8)
                }
            }
            if !release.totalDurationText.isEmpty {
                Text(release.totalDurationText)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .padding(.top, 14)
            }
        }
    }

    private func trackColumn(tracks: [Track], globalOffset: Int) -> some View {
        let height = isCompilation ? rowHeightCompilation : rowHeight
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(tracks.enumerated()), id: \.element.id) {
                localIndex,
                track in
                TrackRowView(
                    track: track,
                    // Core's decision (set only for a compilation); the row does
                    // not re-derive it. `isCompilation` above is kept only for the
                    // album-level row-height heuristic.
                    artist: track.displayArtist,
                    isCurrent: currentTrackId == track.id,
                    isLoading: loadingTrackId == track.id,
                    isPlaying: isPlaying,
                    onPlay: { onPlayFromTrack(globalOffset + localIndex) },
                    onTogglePlayPause: onTogglePlayPause,
                    onAddNext: onAddNext,
                    onAddToQueue: onAddToQueue,
                    onExportTrack: onExportTrack,
                )
                .id(track.id)
                .frame(height: height)
            }
        }
    }
}

#if DEBUG
    @MainActor
    private func previewTrackList(
        albumId: String,
        playFirstTrack: Bool = false
    ) -> some View {
        let detail = PreviewData.releaseDetail(albumId: albumId)
        return AlbumTrackListView(
            release: detail,
            isCompilation: false,
            currentTrackId: playFirstTrack ? detail.tracks.first?.id : nil,
            loadingTrackId: nil,
            isPlaying: playFirstTrack,
            onPlayFromTrack: { _ in },
            onTogglePlayPause: {},
            onAddNext: { _ in },
            onAddToQueue: { _ in },
            onExportTrack: { _ in },
        )
        .padding(24)
        .frame(width: 540)
        .background(Theme.background)
        .environment(UiStore())
    }

    // Single flat side, first track playing.
    #Preview("Track List — Single Disc") {
        previewTrackList(albumId: "a-01", playFirstTrack: true)
            .preferredColorScheme(.dark)
    }

    // Two sides with headers; the long sides split into two columns.
    #Preview("Track List — Two Discs") {
        previewTrackList(albumId: "a-22")
            .preferredColorScheme(.dark)
    }
#endif
