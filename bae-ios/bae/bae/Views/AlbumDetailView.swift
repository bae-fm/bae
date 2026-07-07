import SwiftUI

/// Album detail: a header, a release picker when the album has more than one
/// release, and the selected release's track list grouped by side. Tapping a
/// track plays the release starting there. Reads the shared `LibraryStore`
/// (`albumSummaries` + `releaseDetails`), loading any missing release detail
/// on demand. The store stays live, so sync updates re-render in place.
struct AlbumDetailView: View {
    let albumId: String
    private let context: AlbumDetailContext?

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

    init(
        albumId: String,
        initialReleaseId: String? = nil,
        context: AlbumDetailContext? = nil
    ) {
        self.albumId = albumId
        self.context = context
        _selectedReleaseId = State(initialValue: initialReleaseId)
    }

    var body: some View {
        Group {
            if let context {
                if let releaseId = selectedReleaseId,
                    let detail = libraryStore.releaseDetails[releaseId]
                {
                    content(
                        display: AlbumDetailDisplay(context: context),
                        releasePickerSummary: nil,
                        releaseId: releaseId,
                        detail: detail
                    )
                }
                else {
                    ProgressView()
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            else if let summary = libraryStore.albumSummaries[albumId] {
                let releaseId = activeReleaseId(summary: summary)
                if let detail = libraryStore.releaseDetails[releaseId] {
                    content(
                        display: AlbumDetailDisplay(summary: summary),
                        releasePickerSummary: summary,
                        releaseId: releaseId,
                        detail: detail
                    )
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
            if context != nil {
                if let releaseId = selectedReleaseId {
                    await libraryStore.loadReleaseDetail(
                        releaseId: releaseId,
                        library: library
                    )
                }
                return
            }
            // Eagerly load detail for every release so the picker has labels
            // and switching releases doesn't flash a spinner. N is typically
            // 1-3. Bail on cancellation (the user navigated away).
            guard let summary = libraryStore.albumSummaries[albumId] else {
                if let releaseId = selectedReleaseId {
                    await libraryStore.loadReleaseDetail(
                        releaseId: releaseId,
                        library: library
                    )
                }
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
        display: AlbumDetailDisplay,
        releasePickerSummary: AlbumSummary?,
        releaseId: String,
        detail: ReleaseDetail
    ) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                AlbumDetailHeader(
                    display: display,
                    releaseId: releaseId,
                    detail: detail,
                    showGallery: $showGallery
                )
                ReleaseDownloadSection(releaseId: releaseId, detail: detail)
                if let summary = releasePickerSummary,
                    summary.releaseIds.count > 1
                {
                    releasePicker(summary: summary)
                }
                TrackList(
                    detail: detail,
                    artistDisplay: display.trackArtistDisplay,
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
                loadImage: { item in
                    // The lightbox passes the whole source to the bridge, which
                    // dispatches the read in core. The UI never inspects it.
                    try await mediaPaths.fetchGalleryBytes(
                        releaseId,
                        item.source
                    )
                }
            )
        }
    }

    private func releasePicker(summary: AlbumSummary) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(summary.releaseIds, id: \.self) { id in
                    Button {
                        selectedReleaseId = id
                        Task {
                            await libraryStore.loadReleaseDetail(
                                releaseId: id,
                                library: library
                            )
                        }
                    } label: {
                        Group {
                            if let label = libraryStore.releaseDetails[id]?.displayName {
                                Text(label)
                                    .font(.callout)
                                    .lineLimit(1)
                            }
                            else {
                                ProgressView()
                                    .controlSize(.small)
                            }
                        }
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

    /// The active release: explicit selection, otherwise the album's primary release.
    private func activeReleaseId(summary: AlbumSummary) -> String {
        if let id = selectedReleaseId, summary.releaseIds.contains(id) {
            return id
        }
        precondition(
            summary.releaseIds.contains(summary.primaryReleaseId),
            "primaryReleaseId missing from releaseIds for album \(summary.id)"
        )
        return summary.primaryReleaseId
    }
}

private enum AlbumDetailDisplay {
    case album(AlbumSummary)
    case workRelease(AlbumDetailContext)

    init(summary: AlbumSummary) {
        self = .album(summary)
    }

    init(context: AlbumDetailContext) {
        self = .workRelease(context)
    }

    var title: String {
        switch self {
        case .album(let summary):
            summary.title
        case .workRelease(let context):
            context.title
        }
    }

    var albumMetadata: AlbumDetailAlbumMetadata? {
        switch self {
        case .album(let summary):
            AlbumDetailAlbumMetadata(
                artistNames: summary.artistNames,
                year: summary.year
            )
        case .workRelease:
            nil
        }
    }

    var trackArtistDisplay: TrackArtistDisplay {
        switch self {
        case .album(let summary):
            .album(isCompilation: summary.isCompilation)
        case .workRelease:
            .workRelease
        }
    }
}

struct AlbumDetailContext: Hashable {
    let title: String

    init(workRelease: BridgeWorkReleaseSummary) {
        title = workRelease.albumTitle
    }
}

private struct AlbumDetailAlbumMetadata {
    let artistNames: String
    let year: Int32?
}

private enum TrackArtistDisplay {
    case album(isCompilation: Bool)
    case workRelease

    var showsArtist: Bool {
        switch self {
        case .album(let isCompilation):
            isCompilation
        case .workRelease:
            true
        }
    }
}

private struct AlbumDetailHeader: View {
    let display: AlbumDetailDisplay
    let releaseId: String
    let detail: ReleaseDetail
    @Binding
    var showGallery: Bool

    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            ImageView(imageRef: detail.summary.cover, pointSize: 140)
                .frame(width: 140, height: 140)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .contentShape(Rectangle())
                .onTapGesture {
                    if !detail.galleryItems.isEmpty { showGallery = true }
                }
            VStack(alignment: .leading, spacing: 4) {
                Text(display.title)
                    .font(.title2.bold())
                if let metadata = display.albumMetadata,
                    !metadata.artistNames.isEmpty
                {
                    Text(metadata.artistNames)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                if let year = display.albumMetadata?.year {
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
                playButtons
                    .padding(.top, 8)
                queueButtons
                    .padding(.top, 4)
            }
            Spacer(minLength: 0)
        }
    }

    private var playButtons: some View {
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

    private var queueButtons: some View {
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
}

/// Offline control for the shown release: Download / progress + Cancel /
/// Downloaded + Remove Download. State derives from `ReleaseDownloadStatus`;
/// the queue snapshot and release invalidations keep it live.
private struct ReleaseDownloadSection: View {
    let releaseId: String
    let detail: ReleaseDetail

    @Environment(Downloads.self)
    private var downloads
    @Environment(DownloadStore.self)
    private var downloadStore

    @State
    private var unpinTask: Task<Void, Never>?
    @State
    private var unpinError: String?

    var body: some View {
        let status = ReleaseDownloadStatus(
            pinned: detail.summary.pinned,
            storageActions: detail.storageActions,
            queueState: downloadStore.snapshot.state(forRelease: releaseId)
        )
        VStack(alignment: .leading, spacing: 6) {
            control(status)
            if let unpinError {
                Text(unpinError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
        .onDisappear { unpinTask?.cancel() }
    }

    @ViewBuilder
    private func control(_ status: ReleaseDownloadStatus?) -> some View {
        switch status {
        case nil:
            EmptyView()
        case .available:
            // Fire-and-forget: progress and queue state arrive via the download
            // snapshot. Re-enqueuing is idempotent — core skips ids already
            // queued or pinned.
            actionButton("Download", systemImage: "arrow.down.circle") {
                Task { await downloads.queuePins([releaseId]) }
            }
        case .queued:
            HStack(spacing: 8) {
                WaitingToDownloadLabel()
                cancelButton
            }
        case .downloading(let progress):
            VStack(alignment: .leading, spacing: 6) {
                DownloadTransferProgressView(progress: progress)
                cancelButton
            }
        case .failed(let message):
            VStack(alignment: .leading, spacing: 6) {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.red)
                HStack(spacing: 8) {
                    // Core has no per-item retry: `retryDownloads` flips every
                    // failed entry back to queued, like the macOS Downloads pane.
                    actionButton("Retry", systemImage: "arrow.clockwise") {
                        downloads.retryDownloads()
                    }
                    cancelButton
                }
            }
        case .downloaded:
            downloadedControl
        }
    }

    private var cancelButton: some View {
        actionButton("Cancel", systemImage: "xmark") {
            downloads.cancelDownload(releaseId)
        }
    }

    /// A bordered caption button — the shared shape for every download action.
    /// A `role` (e.g. `.destructive`) drops the accent tint so the role's own
    /// styling shows.
    private func actionButton(
        _ titleKey: LocalizedStringKey,
        systemImage: String,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Label(titleKey, systemImage: systemImage)
                .font(.caption)
        }
        .buttonStyle(.bordered)
        .tint(role == nil ? Theme.accent : nil)
    }

    @ViewBuilder
    private var downloadedControl: some View {
        HStack(spacing: 8) {
            Label("Downloaded", systemImage: "arrow.down.circle.fill")
                .font(.caption)
                .foregroundStyle(.secondary)
            if unpinTask != nil {
                ProgressView()
                    .controlSize(.small)
            }
            else {
                actionButton(
                    "Remove Download",
                    systemImage: "trash",
                    role: .destructive
                ) {
                    removeDownload()
                }
            }
        }
    }

    private func removeDownload() {
        unpinTask?.cancel()
        unpinError = nil
        unpinTask = Task {
            defer { unpinTask = nil }
            do {
                try await downloads.unpinRelease(releaseId)
            }
            catch is CancellationError {
                // View dismissed mid-unpin; core's drop guard emits the
                // terminal ReleaseTransferEnded, which refreshes the release.
            }
            catch {
                unpinError = error.localizedDescription
            }
        }
    }
}

/// Side-grouped track list. Flattens groups to a release-wide index so a tap
/// maps to the ordered list the player builds from the same flattening.
private struct TrackList: View {
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
                        showArtist: artistDisplay.showsArtist,
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
