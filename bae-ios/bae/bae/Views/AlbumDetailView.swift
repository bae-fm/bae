import SwiftUI

/// Album detail: a header, a release picker when the album has more than one
/// release, and the selected release's track list grouped by side. Tapping a
/// track plays the release starting there. Reads the shared `LibraryStore`
/// (`albumSummaries` + `releaseDetails`), loading any missing release detail
/// on demand. The store stays live, so sync updates re-render in place.
struct AlbumDetailView: View {
    let albumId: String

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(Library.self)
    private var library
    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue

    @State
    private var selectedReleaseId: String?
    @State
    private var showGallery = false

    var body: some View {
        Group {
            if let summary = libraryStore.albumSummaries[albumId] {
                let releaseId = activeReleaseId(summary: summary)
                let detail = releaseId.flatMap { libraryStore.releaseDetails[$0] }
                if let releaseId, let detail {
                    content(summary: summary, releaseId: releaseId, detail: detail)
                }
                else {
                    ProgressView()
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Theme.background)
        .navigationTitle("bae")
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom) {
            NowPlayingBar()
        }
        .task(id: albumId) {
            // Eagerly load detail for every release so the picker has labels
            // and switching releases doesn't flash a spinner. N is typically
            // 1-3. Bail on cancellation (the user navigated away).
            guard let summary = libraryStore.albumSummaries[albumId] else {
                return
            }
            for releaseId in summary.releaseIds {
                if Task.isCancelled {
                    return
                }
                await libraryStore.loadReleaseDetail(
                    releaseId: releaseId,
                    library: library
                )
            }
        }
    }

    private func content(
        summary: AlbumSummary,
        releaseId: String,
        detail: ReleaseDetail
    ) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                header(summary: summary, releaseId: releaseId, detail: detail)
                if summary.releaseIds.count > 1 {
                    releasePicker(summary: summary)
                }
                TrackList(
                    detail: detail,
                    isCompilation: summary.isCompilation,
                    onPlayTrackAt: { index in
                        playback.playRelease(releaseId, UInt32(index), false)
                    },
                    onPlayNext: { trackId in queue.addNext([trackId]) },
                    onAddToQueue: { trackId in queue.addToQueue([trackId]) }
                )
            }
            .padding(16)
        }
        .fullScreenCover(isPresented: $showGallery) {
            GalleryView(
                items: detail.galleryItems,
                loadImage: { fileId in
                    try await mediaPaths.fetchGalleryImage(releaseId, fileId)
                }
            )
        }
    }

    private func header(
        summary: AlbumSummary,
        releaseId: String,
        detail: ReleaseDetail
    ) -> some View {
        HStack(alignment: .top, spacing: 16) {
            ImageView(
                path: mediaPaths.imagePathIfExists(releaseId),
                pointSize: 140
            )
            .frame(width: 140, height: 140)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .contentShape(Rectangle())
            .onTapGesture {
                if !detail.galleryItems.isEmpty { showGallery = true }
            }
            VStack(alignment: .leading, spacing: 4) {
                Text(summary.title)
                    .font(.title2.bold())
                Text(summary.artistNames)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                if let year = summary.year {
                    Text(String(year))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if !detail.compactMetadata.isEmpty {
                    Text(detail.compactMetadata)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(.top, 4)
                }
                playButtons(releaseId: releaseId)
                    .padding(.top, 8)
                queueButtons(releaseId: releaseId)
                    .padding(.top, 4)
            }
            Spacer(minLength: 0)
        }
    }

    private func playButtons(releaseId: String) -> some View {
        HStack(spacing: 8) {
            Button {
                playback.playRelease(releaseId, nil, false)
            } label: {
                Label("Play", systemImage: "play.fill")
            }
            .buttonStyle(.borderedProminent)
            Button {
                playback.playRelease(releaseId, nil, true)
            } label: {
                Label("Shuffle", systemImage: "shuffle")
            }
            .buttonStyle(.bordered)
        }
    }

    private func queueButtons(releaseId: String) -> some View {
        HStack(spacing: 8) {
            Button {
                queue.addReleaseNext(releaseId)
            } label: {
                Label("Play Next", systemImage: "text.insert")
                    .font(.caption)
            }
            Button {
                queue.addReleaseToQueue(releaseId)
            } label: {
                Label("Add to Queue", systemImage: "text.append")
                    .font(.caption)
            }
        }
        .buttonStyle(.bordered)
        .tint(Theme.accent)
    }

    private func releasePicker(summary: AlbumSummary) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(summary.releaseIds, id: \.self) { id in
                    let label =
                        libraryStore.releaseDetails[id]?.displayName ?? "Release"
                    Button {
                        selectedReleaseId = id
                        Task {
                            await libraryStore.loadReleaseDetail(
                                releaseId: id,
                                library: library
                            )
                        }
                    } label: {
                        Text(label)
                            .font(.callout)
                            .lineLimit(1)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(
                                RoundedRectangle(cornerRadius: 16)
                                    .fill(
                                        id == activeReleaseId(summary: summary)
                                            ? Theme.accentSoft
                                            : Theme.surfaceElevated
                                    )
                            )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    /// The active release: explicit selection → primary → first.
    private func activeReleaseId(summary: AlbumSummary) -> String? {
        if let id = selectedReleaseId, summary.releaseIds.contains(id) {
            return id
        }
        if summary.releaseIds.contains(summary.primaryReleaseId) {
            return summary.primaryReleaseId
        }
        return summary.releaseIds.first
    }
}

/// Side-grouped track list. Flattens groups to a release-wide index so a tap
/// maps to the ordered list the player builds from the same flattening.
private struct TrackList: View {
    let detail: ReleaseDetail
    let isCompilation: Bool
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
                if !group.sideLabel.isEmpty {
                    Text(group.sideLabel)
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
                        showArtist: isCompilation,
                        onPlay: { onPlayTrackAt(groupOffset + localIndex) },
                        onPlayNext: { onPlayNext(track.id) },
                        onAddToQueue: { onAddToQueue(track.id) }
                    )
                }
            }
            if !detail.totalDurationLabel.isEmpty {
                Text(detail.totalDurationLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.top, 12)
            }
        }
    }
}

private struct TrackRow: View {
    let track: Track
    let showArtist: Bool
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
                playback.togglePlayPause()
            }
            else {
                onPlay()
            }
        } label: {
            HStack(spacing: 12) {
                // Both stay in the layout tree, toggled by opacity, so swapping
                // the current row in/out never re-measures the stack.
                ZStack(alignment: .leading) {
                    Text(track.positionLabel.isEmpty ? "-" : track.positionLabel)
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
                    if showArtist {
                        Text(track.artistNames)
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
